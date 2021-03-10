use crate::errors::Error;
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
    //链接hash值再次hash
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
        let mut height = 0;
        while self.tree_width(height) > 1 {
            height += 1;
        }
        height
    }
    fn calc_hasher(
        &mut self,
        height: usize,
        pos: usize,
        ids: &Vec<Hasher>,
    ) -> Result<Hasher, Error> {
        if ids.len() != self.trans {
            return Err(Error::BadMerkleTree);
        }
        if height == 0 {
            return Ok(ids[pos].clone());
        }
        let (left, mut right) = (self.calc_hasher(height - 1, pos * 2, ids)?, Hasher::zero());
        if pos * 2 + 1 < self.tree_width(height - 1) {
            right = self.calc_hasher(height - 1, pos * 2 + 1, ids)?;
        } else {
            right = left.clone();
        }
        return Ok(Self::hash(&left, &right));
    }
    fn build(
        &mut self,
        height: usize,
        pos: usize,
        ids: &Vec<Hasher>,
        vb: &Vec<bool>,
    ) -> Result<(), Error> {
        let mut bmatch = false;
        let mut p = pos << height;
        while p < (pos + 1) << height && p < self.trans {
            if vb[p] {
                bmatch = true
            }
            p += 1;
        }
        self.bits.push(bmatch);
        if height == 0 || !bmatch {
            let hv = self.calc_hasher(height, pos, ids)?;
            self.vhash.push(hv);
        } else {
            self.build(height - 1, pos * 2, ids, vb)?;
            if pos * 2 + 1 < self.tree_width(height - 1) {
                self.build(height - 1, pos * 2 + 1, ids, vb)?;
            }
        }
        Ok(())
    }
    /// 计算默克尔树Hash
    pub fn compute(nodes: &Vec<Hasher>) -> Result<Hasher, Error> {
        let mut merkle = MerkleTree::new(nodes)?;
        merkle.extract_root()
    }
    ///根据hash数组创建默克尔树
    fn new(ids: &Vec<Hasher>) -> Result<Self, Error> {
        let mut result = MerkleTree {
            trans: ids.len(),
            vhash: vec![],
            bits: vec![],
            bad: false,
        };
        let height = result.tree_height();
        let mut vb: Vec<bool> = vec![false; result.trans];
        result.build(height, 0, ids, &mut vb)?;
        Ok(result)
    }
    fn extract(
        &mut self,
        height: usize,
        pos: usize,
        nbits: &mut usize,
        nhash: &mut usize,
        ids: &mut Vec<Hasher>,
        idx: &mut Vec<usize>,
    ) -> Hasher {
        if *nbits >= self.bits.len() {
            self.bad = true;
            return Hasher::default();
        }
        let bmatch = self.bits[*nbits];
        *nbits += 1;
        if height == 0 || !bmatch {
            if *nhash >= self.vhash.len() {
                self.bad = true;
                return Hasher::default();
            }
            let hash = &self.vhash[*nhash];
            *nhash += 1;
            if height == 0 && bmatch {
                ids.push(hash.clone());
                idx.push(pos);
            }
            return hash.clone();
        }
        let (left, mut right) = (
            self.extract(height - 1, pos * 2, nbits, nhash, ids, idx),
            Hasher::zero(),
        );
        if pos * 2 + 1 < self.tree_width(height - 1) {
            right = self.extract(height - 1, pos * 2 + 1, nbits, nhash, ids, idx);
            if left == right {
                self.bad = true;
            }
        } else {
            right = left.clone();
        }
        Self::hash(&left, &right)
    }
    /// 计算默克尔树hash值
    pub fn extract_root(&mut self) -> Result<Hasher, Error> {
        let mut ids: Vec<Hasher> = vec![];
        let mut idx: Vec<usize> = vec![];
        self.bad = false;
        if self.trans == 0 {
            return Err(Error::BadMerkleTree);
        }
        if self.vhash.len() > self.trans {
            return Err(Error::BadMerkleTree);
        }
        if self.bits.len() < self.vhash.len() {
            return Err(Error::BadMerkleTree);
        }
        let (mut nbits, mut nhash, height) = (0, 0, self.tree_height());
        let root = self.extract(height, 0, &mut nbits, &mut nhash, &mut ids, &mut idx);
        if self.bad {
            return Err(Error::BadMerkleTree);
        }
        if (nbits + 7) / 8 != (self.bits.len() + 7 / 8) {
            return Err(Error::BadMerkleTree);
        }
        if nhash != self.vhash.len() {
            return Err(Error::BadMerkleTree);
        }
        return Ok(root);
    }
}

#[test]
fn test_merkle1() {
    use std::convert::TryFrom;
    let mut ids: Vec<Hasher> = vec![];
    for i in 0..7 {
        let hv = Hasher::hash(&[i]);
        ids.push(hv);
    }
    let hv = MerkleTree::compute(&ids).unwrap();
    let hm = Hasher::try_from("4da57c69139fb1b4d2ebccb63f239fe9d46aaf94abbec97129c7cd577d5ce67d")
        .unwrap();
    assert_eq!(hv, hm);
}

#[test]
fn test_merkle2() {
    use std::convert::TryFrom;
    let mut ids: Vec<Hasher> = vec![];
    for i in 10..20 {
        let hv = Hasher::hash(&[i]);
        ids.push(hv);
    }
    let hv = MerkleTree::compute(&ids).unwrap();
    let hm = Hasher::try_from("01b67d53db24394cd5640a7552570523f216ec7492680cc96ff0b8235a5346d0")
        .unwrap();
    assert_eq!(hv, hm);
}
