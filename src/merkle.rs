use crate::hasher::Hasher;
use crate::iobuf::Writer;
///默克尔树
#[derive(Debug)]
pub struct MerkleTree {
    trans: usize,
    vhash: Vec<Hasher>,
    bits: Vec<bool>,
    bad: bool,
}

impl MerkleTree {
    ///create merkle tree
    pub fn new(num: usize) -> Self {
        MerkleTree {
            trans: num,
            vhash: vec![],
            bits: vec![],
            bad: true,
        }
    }

    fn hash(h1: &Hasher, h2: &Hasher) -> Hasher {
        let mut w = Writer::default();
        w.put_bytes(h1.to_bytes());
        w.put_bytes(h2.to_bytes());
        Hasher::hash(w.bytes())
    }

    fn tree_width(&self, h: usize) -> usize {
        (self.trans + (1 << h) - 1) >> h
    }

    fn tree_height(&self) -> usize {
        let mut h = 0;
        while self.tree_width(h) > 1 {
            h += 1;
        }
        h
    }
}
