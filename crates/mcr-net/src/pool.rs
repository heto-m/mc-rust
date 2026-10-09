use crossbeam_channel::{Receiver, Sender, bounded};
use std::fmt;
use std::ops::Deref;

const MAX_KEPT_CAPACITY: usize = 1 << 20;

#[derive(Clone)]
pub struct BufPool {
    tx: Sender<Vec<u8>>,
    rx: Receiver<Vec<u8>>,
}

impl BufPool {
    pub fn new(capacity: usize) -> Self {
        let (tx, rx) = bounded(capacity);
        Self { tx, rx }
    }

    pub fn get(&self) -> Vec<u8> {
        let mut v = self.rx.try_recv().unwrap_or_default();
        v.clear();
        v
    }

    pub fn put(&self, v: Vec<u8>) {
        if v.capacity() <= MAX_KEPT_CAPACITY {
            let _ = self.tx.try_send(v);
        }
    }

    pub fn pooled(&self, buf: Vec<u8>, offset: usize) -> PooledBuf {
        PooledBuf {
            buf,
            offset,
            home: Some(self.tx.clone()),
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
    home: Option<Sender<Vec<u8>>>,
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
        if let Some(home) = &self.home
            && self.buf.capacity() <= MAX_KEPT_CAPACITY
        {
            let _ = home.try_send(std::mem::take(&mut self.buf));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
