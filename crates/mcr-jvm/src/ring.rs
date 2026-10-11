use std::alloc::{Layout, alloc_zeroed, dealloc};
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

pub const RING_MAGIC: u32 = u32::from_be_bytes(*b"MCRR");
pub const RING_HEADER_SIZE: usize = 192;
pub const RING_WRITE_OFFSET: usize = 64;
pub const RING_READ_OFFSET: usize = 128;
pub const RECORD_HEADER_SIZE: usize = 8;
pub const PADDING_KIND: u16 = u16::MAX;
pub const MIN_CAPACITY: usize = 256;
pub const MAX_CAPACITY: usize = 1 << 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RingError {
    #[error("ring capacity {0} must be a power of two in {MIN_CAPACITY}..={MAX_CAPACITY}")]
    BadCapacity(usize),
    #[error("ring allocation failed")]
    Alloc,
    #[error("corrupt ring record at offset {offset} (len {len})")]
    Corrupt { offset: usize, len: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PushError {
    #[error("ring full")]
    Full,
    #[error("payload of {len} bytes exceeds the ring maximum of {max}")]
    TooLarge { len: usize, max: usize },
    #[error("message kind {PADDING_KIND:#x} is reserved")]
    ReservedKind,
}

/// SPSC byte ring shared with Java by address; the layout constants are mirrored in `Abi.java`.
#[derive(Debug)]
pub struct Ring {
    base: NonNull<u8>,
    capacity: usize,
    writer_taken: AtomicBool,
    reader_taken: AtomicBool,
}

// SAFETY: shared state is accessed through atomics; the data region is partitioned between the single
// writer and single reader by the release/acquire positions.
unsafe impl Send for Ring {}
// SAFETY: as above.
unsafe impl Sync for Ring {}

impl Ring {
    pub fn new(capacity: usize) -> Result<Arc<Self>, RingError> {
        if !capacity.is_power_of_two() || !(MIN_CAPACITY..=MAX_CAPACITY).contains(&capacity) {
            return Err(RingError::BadCapacity(capacity));
        }
        let layout = Self::layout(capacity);
        // SAFETY: the layout has a non-zero size.
        let base = NonNull::new(unsafe { alloc_zeroed(layout) }).ok_or(RingError::Alloc)?;
        let ring = Self {
            base,
            capacity,
            writer_taken: AtomicBool::new(false),
            reader_taken: AtomicBool::new(false),
        };
        ring.word(0).store(RING_MAGIC, Ordering::Relaxed);
        ring.word(4).store(capacity as u32, Ordering::Release);
        Ok(Arc::new(ring))
    }

    fn layout(capacity: usize) -> Layout {
        Layout::from_size_align(RING_HEADER_SIZE + capacity, 64).expect("bounded ring layout")
    }

    /// Address handed to Java; valid for as long as the ring lives.
    pub fn addr(&self) -> i64 {
        self.base.as_ptr() as i64
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn max_payload(&self) -> usize {
        self.capacity / 2 - RECORD_HEADER_SIZE
    }

    pub fn writer(self: &Arc<Self>) -> Option<RingWriter> {
        if self.writer_taken.swap(true, Ordering::AcqRel) {
            return None;
        }
        Some(RingWriter {
            ring: self.clone(),
            write: self.write_pos().load(Ordering::Relaxed),
            read_cache: self.read_pos().load(Ordering::Acquire),
        })
    }

    pub fn reader(self: &Arc<Self>) -> Option<RingReader> {
        if self.reader_taken.swap(true, Ordering::AcqRel) {
            return None;
        }
        Some(RingReader {
            ring: self.clone(),
            read: self.read_pos().load(Ordering::Relaxed),
            write_cache: self.write_pos().load(Ordering::Acquire),
        })
    }

    fn word(&self, offset: usize) -> &AtomicU32 {
        // SAFETY: header offsets are in bounds and 4-aligned; the memory lives as long as `self`.
        unsafe { &*self.base.as_ptr().add(offset).cast::<AtomicU32>() }
    }

    fn write_pos(&self) -> &AtomicU64 {
        // SAFETY: offset 64 is in bounds and 8-aligned.
        unsafe {
            &*self
                .base
                .as_ptr()
                .add(RING_WRITE_OFFSET)
                .cast::<AtomicU64>()
        }
    }

    fn read_pos(&self) -> &AtomicU64 {
        // SAFETY: offset 128 is in bounds and 8-aligned.
        unsafe { &*self.base.as_ptr().add(RING_READ_OFFSET).cast::<AtomicU64>() }
    }

    fn data(&self) -> *mut u8 {
        // SAFETY: the data region starts right after the header.
        unsafe { self.base.as_ptr().add(RING_HEADER_SIZE) }
    }

    fn mask(&self) -> u64 {
        self.capacity as u64 - 1
    }
}

impl Drop for Ring {
    fn drop(&mut self) {
        // SAFETY: allocated in `new` with the same layout.
        unsafe { dealloc(self.base.as_ptr(), Self::layout(self.capacity)) };
    }
}

fn record_size(len: usize) -> usize {
    (RECORD_HEADER_SIZE + len + 7) & !7
}

#[derive(Debug)]
pub struct RingWriter {
    ring: Arc<Ring>,
    write: u64,
    read_cache: u64,
}

impl RingWriter {
    pub fn ring(&self) -> &Arc<Ring> {
        &self.ring
    }

    pub fn try_push(&mut self, kind: u16, payload: &[u8]) -> Result<(), PushError> {
        self.try_push_with(kind, payload.len(), |buf| buf.copy_from_slice(payload))
    }

    pub fn try_push_with(
        &mut self,
        kind: u16,
        len: usize,
        fill: impl FnOnce(&mut [u8]),
    ) -> Result<(), PushError> {
        if kind == PADDING_KIND {
            return Err(PushError::ReservedKind);
        }
        let max = self.ring.max_payload();
        if len > max {
            return Err(PushError::TooLarge { len, max });
        }
        let cap = self.ring.capacity;
        let total = record_size(len);
        let pos = (self.write & self.ring.mask()) as usize;
        let tail = cap - pos;
        let needed = if total <= tail { total } else { tail + total };
        if needed as u64 > cap as u64 - (self.write - self.read_cache) {
            self.read_cache = self.ring.read_pos().load(Ordering::Acquire);
            if needed as u64 > cap as u64 - (self.write - self.read_cache) {
                return Err(PushError::Full);
            }
        }
        let data = self.ring.data();
        let mut pos = pos;
        if total > tail {
            // SAFETY: `pos + 8 <= cap` because positions and capacity are 8-aligned and tail > 0.
            unsafe {
                write_header(
                    data.add(pos),
                    (tail - RECORD_HEADER_SIZE) as u32,
                    PADDING_KIND,
                )
            };
            self.write += tail as u64;
            pos = 0;
        }
        // SAFETY: the record [pos, pos + total) lies in the data region and is free (checked above), so
        // the reader does not touch it until the release store below.
        unsafe {
            let rec = data.add(pos);
            write_header(rec, len as u32, kind);
            fill(std::slice::from_raw_parts_mut(
                rec.add(RECORD_HEADER_SIZE),
                len,
            ));
        }
        self.write += total as u64;
        self.ring.write_pos().store(self.write, Ordering::Release);
        Ok(())
    }
}

impl Drop for RingWriter {
    fn drop(&mut self) {
        self.ring.writer_taken.store(false, Ordering::Release);
    }
}

/// # Safety
/// `rec` must point to 8 writable, 4-aligned bytes.
unsafe fn write_header(rec: *mut u8, len: u32, kind: u16) {
    // SAFETY: guaranteed by the caller.
    unsafe {
        rec.cast::<u32>().write(len);
        rec.add(4).cast::<u16>().write(kind);
        rec.add(6).cast::<u16>().write(0);
    }
}

#[derive(Debug)]
pub struct RingReader {
    ring: Arc<Ring>,
    read: u64,
    write_cache: u64,
}

impl RingReader {
    pub fn ring(&self) -> &Arc<Ring> {
        &self.ring
    }

    /// Next message, or `None` when empty. The message is consumed when the returned value drops.
    pub fn try_pop(&mut self) -> Result<Option<Message<'_>>, RingError> {
        let cap = self.ring.capacity;
        loop {
            if self.read == self.write_cache {
                self.write_cache = self.ring.write_pos().load(Ordering::Acquire);
                if self.read == self.write_cache {
                    return Ok(None);
                }
            }
            let pos = (self.read & self.ring.mask()) as usize;
            // SAFETY: pos is 8-aligned inside the data region and the writer published this header.
            let (len, kind) = unsafe {
                let rec = self.ring.data().add(pos);
                (rec.cast::<u32>().read(), rec.add(4).cast::<u16>().read())
            };
            let total = record_size(len as usize);
            let available = self.write_cache - self.read;
            let fits =
                (len as usize) <= cap - pos - RECORD_HEADER_SIZE && total as u64 <= available;
            let sane = kind == PADDING_KIND || len as usize <= self.ring.max_payload();
            if !fits || !sane {
                return Err(RingError::Corrupt { offset: pos, len });
            }
            if kind == PADDING_KIND {
                self.advance(total);
                continue;
            }
            return Ok(Some(Message {
                offset: pos + RECORD_HEADER_SIZE,
                len: len as usize,
                total,
                kind,
                reader: self,
            }));
        }
    }

    fn advance(&mut self, total: usize) {
        self.read += total as u64;
        self.ring.read_pos().store(self.read, Ordering::Release);
    }
}

impl Drop for RingReader {
    fn drop(&mut self) {
        self.ring.reader_taken.store(false, Ordering::Release);
    }
}

#[derive(Debug)]
pub struct Message<'a> {
    reader: &'a mut RingReader,
    kind: u16,
    offset: usize,
    len: usize,
    total: usize,
}

impl Message<'_> {
    pub fn kind(&self) -> u16 {
        self.kind
    }

    pub fn payload(&self) -> &[u8] {
        // SAFETY: the record was validated in `try_pop` and the writer cannot reuse it before `drop`.
        unsafe { std::slice::from_raw_parts(self.reader.ring.data().add(self.offset), self.len) }
    }
}

impl Drop for Message<'_> {
    fn drop(&mut self) {
        self.reader.advance(self.total);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
    }

    #[test]
    fn rejects_bad_capacities() {
        for cap in [0, 100, 128, 3 << 10, MAX_CAPACITY * 2] {
            assert_eq!(Ring::new(cap).unwrap_err(), RingError::BadCapacity(cap));
        }
    }

    #[test]
    fn header_layout_for_java() {
        let ring = Ring::new(1024).unwrap();
        assert_eq!(ring.addr() % 64, 0);
        assert_eq!(ring.word(0).load(Ordering::Relaxed), 0x4D43_5252);
        assert_eq!(ring.word(4).load(Ordering::Relaxed), 1024);
    }

    #[test]
    fn single_side_handles() {
        let ring = Ring::new(1024).unwrap();
        let w = ring.writer().unwrap();
        assert!(ring.writer().is_none());
        drop(w);
        assert!(ring.writer().is_some());
        let _r = ring.reader().unwrap();
        assert!(ring.reader().is_none());
    }

    #[test]
    fn push_pop_and_limits() {
        let ring = Ring::new(256).unwrap();
        let mut w = ring.writer().unwrap();
        let mut r = ring.reader().unwrap();
        assert!(r.try_pop().unwrap().is_none());
        w.try_push(7, b"hello").unwrap();
        w.try_push(8, b"").unwrap();
        {
            let m = r.try_pop().unwrap().unwrap();
            assert_eq!((m.kind(), m.payload()), (7, &b"hello"[..]));
        }
        {
            let m = r.try_pop().unwrap().unwrap();
            assert_eq!((m.kind(), m.payload().len()), (8, 0));
        }
        assert_eq!(ring.max_payload(), 120);
        assert_eq!(
            w.try_push(1, &[0; 121]),
            Err(PushError::TooLarge { len: 121, max: 120 })
        );
        assert_eq!(w.try_push(PADDING_KIND, b""), Err(PushError::ReservedKind));
    }

    #[test]
    fn full_ring_refuses_then_accepts_after_pop() {
        let ring = Ring::new(256).unwrap();
        let mut w = ring.writer().unwrap();
        let mut r = ring.reader().unwrap();
        let mut pushed = 0;
        while w.try_push(1, &[0xAB; 56]).is_ok() {
            pushed += 1;
        }
        assert_eq!(pushed, 4);
        assert_eq!(w.try_push(1, &[0; 1]), Err(PushError::Full));
        drop(r.try_pop().unwrap().unwrap());
        w.try_push(1, &[0xCD; 56]).unwrap();
    }

    #[test]
    fn records_never_wrap() {
        let ring = Ring::new(256).unwrap();
        let mut w = ring.writer().unwrap();
        let mut r = ring.reader().unwrap();
        for round in 0..100u8 {
            let len = 40 + usize::from(round % 80);
            w.try_push(u16::from(round), &vec![round; len]).unwrap();
            let m = r.try_pop().unwrap().unwrap();
            assert_eq!(m.kind(), u16::from(round));
            assert!(m.payload().iter().all(|&b| b == round));
            assert_eq!(m.payload().len(), len);
        }
    }

    #[test]
    fn corrupt_length_is_reported() {
        let ring = Ring::new(256).unwrap();
        let mut w = ring.writer().unwrap();
        let mut r = ring.reader().unwrap();
        w.try_push(1, b"abc").unwrap();
        // SAFETY: test-only header overwrite inside the data region.
        unsafe { ring.data().cast::<u32>().write(10_000) };
        assert!(matches!(
            r.try_pop(),
            Err(RingError::Corrupt {
                offset: 0,
                len: 10_000
            })
        ));
    }

    #[test]
    fn two_thread_stress_preserves_order_and_bytes() {
        const MESSAGES: u64 = 200_000;
        let ring = Ring::new(4096).unwrap();
        let max = ring.max_payload() as u64;
        let mut w = ring.writer().unwrap();
        let producer = std::thread::spawn(move || {
            let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
            let mut buf = Vec::new();
            for i in 0..MESSAGES {
                let len = (rng.next() % (max + 1)) as usize;
                buf.clear();
                buf.extend((0..len).map(|j| (i as usize + j) as u8));
                let kind = (i % 1000) as u16;
                loop {
                    match w.try_push(kind, &buf) {
                        Ok(()) => break,
                        Err(PushError::Full) => std::hint::spin_loop(),
                        Err(e) => panic!("{e}"),
                    }
                }
            }
        });
        let mut r = ring.reader().unwrap();
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        let mut i = 0;
        while i < MESSAGES {
            let Some(m) = r.try_pop().unwrap() else {
                std::hint::spin_loop();
                continue;
            };
            let len = (rng.next() % (max + 1)) as usize;
            assert_eq!(m.kind(), (i % 1000) as u16);
            assert_eq!(m.payload().len(), len);
            assert!(
                m.payload()
                    .iter()
                    .enumerate()
                    .all(|(j, &b)| b == (i as usize + j) as u8)
            );
            i += 1;
        }
        producer.join().unwrap();
        assert!(r.try_pop().unwrap().is_none());
    }
}
