#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dest {
    Cube,
    Model,
    Sections,
}

#[derive(Debug)]
struct Op {
    dest: Dest,
    dst: u64,
    src: usize,
    len: usize,
}

#[derive(Debug, Default)]
pub(crate) struct Flushed {
    pub writes: usize,
    pub bytes: usize,
}

/// CPU-side upload ring: writes accumulate in one reused byte vector, are grouped into atomic
/// batches (a section's quads plus its metadata), and are coalesced when adjacent at flush time.
#[derive(Debug, Default)]
pub(crate) struct Uploader {
    data: Vec<u8>,
    ops: Vec<Op>,
    batch_ends: Vec<usize>,
}

impl Uploader {
    pub fn push(&mut self, dest: Dest, dst: u64, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        self.ops.push(Op {
            dest,
            dst,
            src: self.data.len(),
            len: bytes.len(),
        });
        self.data.extend_from_slice(bytes);
    }

    pub fn end_batch(&mut self) {
        if self.batch_ends.last() != Some(&self.ops.len()) && !self.ops.is_empty() {
            self.batch_ends.push(self.ops.len());
        }
    }

    pub fn pending_bytes(&self) -> usize {
        self.data.len()
    }

    /// Emits whole batches until `budget` bytes are exceeded (always at least one batch).
    pub fn flush(&mut self, budget: usize, mut write: impl FnMut(Dest, u64, &[u8])) -> Flushed {
        self.end_batch();
        let mut take_ops = 0;
        let mut bytes = 0;
        for &end in &self.batch_ends {
            let batch_bytes: usize = self.ops[take_ops..end].iter().map(|o| o.len).sum();
            if take_ops > 0 && bytes + batch_bytes > budget {
                break;
            }
            bytes += batch_bytes;
            take_ops = end;
        }

        let mut writes = 0;
        let mut i = 0;
        while i < take_ops {
            let first = &self.ops[i];
            let (dest, dst, src) = (first.dest, first.dst, first.src);
            let mut len = first.len;
            i += 1;
            while i < take_ops {
                let next = &self.ops[i];
                if next.dest != dest || next.dst != dst + len as u64 || next.src != src + len {
                    break;
                }
                len += next.len;
                i += 1;
            }
            write(dest, dst, &self.data[src..src + len]);
            writes += 1;
        }

        if take_ops == self.ops.len() {
            self.ops.clear();
            self.data.clear();
            self.batch_ends.clear();
        } else {
            let data_cut = self.ops[take_ops].src;
            self.ops.drain(..take_ops);
            self.data.drain(..data_cut);
            for op in &mut self.ops {
                op.src -= data_cut;
            }
            self.batch_ends.retain(|&e| e > take_ops);
            for e in &mut self.batch_ends {
                *e -= take_ops;
            }
        }
        Flushed { writes, bytes }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coalesces_adjacent_writes() {
        let mut u = Uploader::default();
        u.push(Dest::Cube, 0, &[1, 2]);
        u.push(Dest::Cube, 2, &[3]);
        u.push(Dest::Cube, 10, &[4]);
        u.push(Dest::Model, 3, &[5]);
        let mut out = Vec::new();
        let f = u.flush(usize::MAX, |d, o, b| out.push((d, o, b.to_vec())));
        assert_eq!(f.writes, 3);
        assert_eq!(out[0], (Dest::Cube, 0, vec![1, 2, 3]));
        assert_eq!(u.pending_bytes(), 0);
    }

    #[test]
    fn budget_splits_on_batch_boundaries() {
        let mut u = Uploader::default();
        for i in 0..3u64 {
            u.push(Dest::Cube, i * 100, &[0; 10]);
            u.push(Dest::Sections, i * 64, &[0; 10]);
            u.end_batch();
        }
        let mut n = 0;
        let f = u.flush(25, |_, _, _| n += 1);
        assert_eq!((f.bytes, n), (20, 2));
        assert_eq!(u.pending_bytes(), 40);
        let f = u.flush(25, |_, _, _| {});
        assert_eq!(f.bytes, 20);
        assert_eq!(u.pending_bytes(), 20);
    }
}
