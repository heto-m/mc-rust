use crossbeam_channel::{Receiver, Sender, bounded};
use std::fmt;
use std::ops::Deref;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

const MAX_KEPT_CAPACITY: usize = 1 << 20;
/// Idle capacity a pool keeps in all; a burst of large chunk packets is not held for the session.
pub const MAX_IDLE_BYTES: usize = 8 << 20;

#[derive(Clone)]
pub struct BufPool {
    tx: Sender<Vec<u8>>,
    rx: Receiver<Vec<u8>>,
    idle: Arc<AtomicUsize>,
}

impl BufPool {
    pub fn new(capacity: usize) -> Self {
        let (tx, rx) = bounded(capacity);
        Self {
            tx,
            rx,
            idle: Arc::default(),
        }
    }

    pub fn get(&self) -> Vec<u8> {
        let mut v = self.rx.try_recv().unwrap_or_default();
        self.idle.fetch_sub(v.capacity(), Ordering::AcqRel);
        v.clear();
        v
    }

    pub fn put(&self, v: Vec<u8>) {
        let cap = v.capacity();
        if cap > MAX_KEPT_CAPACITY {
            return;
        }
        let reserved = reserve(&self.idle, cap, MAX_IDLE_BYTES);
        if reserved && self.tx.try_send(v).is_err() {
            self.idle.fetch_sub(cap, Ordering::AcqRel);
        }
    }

    pub fn idle_bytes(&self) -> usize {
        self.idle.load(Ordering::Acquire)
    }

    pub fn pooled(&self, buf: Vec<u8>, offset: usize) -> PooledBuf {
        PooledBuf {
            buf,
            offset,
            home: Some(self.clone()),
        }
    }
}

fn reserve(counter: &AtomicUsize, n: usize, max: usize) -> bool {
    let mut current = counter.load(Ordering::Acquire);
    loop {
        if current + n > max {
            return false;
        }
        match counter.compare_exchange_weak(
            current,
            current + n,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => return true,
            Err(seen) => current = seen,
        }
    }
}

impl fmt::Debug for BufPool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BufPool")
            .field("idle", &self.rx.len())
            .finish()
    }
}

/// Owned bytes that return their allocation to the pool when dropped.
pub struct PooledBuf {
    buf: Vec<u8>,
    offset: usize,
    home: Option<BufPool>,
}

impl PooledBuf {
    pub fn detached(bytes: Vec<u8>) -> Self {
        Self {
            buf: bytes,
            offset: 0,
            home: None,
        }
    }
}

impl Deref for PooledBuf {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.buf[self.offset..]
    }
}

impl fmt::Debug for PooledBuf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PooledBuf")
            .field("len", &self.len())
            .finish()
    }
}

impl Drop for PooledBuf {
    fn drop(&mut self) {
        if let Some(home) = &self.home {
            home.put(std::mem::take(&mut self.buf));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_capacity_is_bounded_in_bytes() {
        let pool = BufPool::new(256);
        for _ in 0..1000 {
            pool.put(Vec::with_capacity(1 << 20));
            drop(pool.pooled(Vec::with_capacity(512 << 10), 0));
        }
        assert!(pool.idle_bytes() <= MAX_IDLE_BYTES);
        for _ in 0..300 {
            drop(pool.get());
        }
        assert_eq!(pool.idle_bytes(), 0);
    }

    #[test]
    fn buffers_are_recycled() {
        let pool = BufPool::new(4);
        let mut v = pool.get();
        v.extend_from_slice(&[1, 2, 3, 4]);
        let ptr = v.as_ptr();
        let p = pool.pooled(v, 1);
        assert_eq!(&*p, &[2, 3, 4]);
        drop(p);
        let again = pool.get();
        assert!(again.is_empty());
        assert_eq!(again.as_ptr(), ptr);
    }
}
