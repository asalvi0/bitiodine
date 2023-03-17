use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use crate::block::Block;
use crate::hash::ZERO_HASH;
use crate::hash160::Hash160;
use crate::preamble::*;
use crate::transactions::{Transaction, TransactionInput, TransactionOutput};
use crate::visitors::BlockChainVisitor;
use crate::{Address, HighLevel};

pub struct Clusterizer {
    clusters: DisjointSet<Address>,
}

/// Tarjan's Union-Find data structure.
pub struct DisjointSet<T: Clone + Hash + Eq> {
    set_size: usize,
    parent: Vec<usize>,
    rank: Vec<usize>,
    map: HashMap<T, usize>, // Each T entry is mapped onto a usize tag.
}

const OUTPUT_STRING_CAPACITY: usize = 100usize * 234000000usize;

impl<T> DisjointSet<T>
where
    T: Clone + Hash + Eq,
{
    pub fn new() -> Self {
        const CAPACITY: usize = 1000000;
        DisjointSet {
            set_size: 0,
            parent: Vec::with_capacity(CAPACITY),
            rank: Vec::with_capacity(CAPACITY),
            map: HashMap::with_capacity(CAPACITY),
        }
    }

    pub fn size(&self) -> usize {
        self.set_size
    }

    pub fn make_set(&mut self, x: T) {
        if self.map.contains_key(&x) {
            return;
        }

        let len = &mut self.set_size;
        self.map.insert(x, *len);
        self.parent.push(*len);
        self.rank.push(0);

        *len += 1;
    }

    /// Returns Some(num), num is the tag of subset in which x is.
    /// If x is not in the data structure, it returns None.
    pub fn find(&mut self, x: &T) -> Option<usize> {
        let pos: usize;
        match self.map.get(x) {
            Some(p) => {
                pos = *p;
            }
            None => return None,
        }

        let ret = DisjointSet::<T>::find_internal(&mut self.parent, pos);
        Some(ret)
    }

    /// Implements path compression.
    fn find_internal(p: &mut Vec<usize>, n: usize) -> usize {
        if p[n] != n {
            let parent = p[n];
            p[n] = DisjointSet::<T>::find_internal(p, parent);
            p[n]
        } else {
            n
        }
    }

    /// Union the subsets to which x and y belong.
    /// If it returns Ok<u32>, it is the tag for unified subset.
    /// If it returns Err(), at least one of x and y is not in the disjoint-set.
    pub fn union(&mut self, x: &T, y: &T) -> Result<usize> {
        let mut find = |item: &T| -> Result<(usize, usize)> {
            match self.find(item) {
                Some(item_rank) => Ok((item_rank, self.rank[item_rank])),
                None => Err(EofError),
            }
        };
        let (x_root, x_rank) = find(x)?;
        let (y_root, y_rank) = find(y)?;

        {
            // let x_root;
            // let x_rank;
            // let y_root;
            // let y_rank;

            // match self.find(&x) {
            //     Some(x_r) => {
            //         x_root = x_r;
            //         x_rank = self.rank[x_root];
            //     }
            //     None => {
            //         return Err(());
            //     }
            // }

            // match self.find(&y) {
            //     Some(y_r) => {
            //         y_root = y_r;
            //         y_rank = self.rank[y_root];
            //     }
            //     None => {
            //         return Err(());
            //     }
            // }
        }

        // Implements union-by-rank optimization.
        if x_root == y_root {
            return Ok(x_root);
        }

        if x_rank > y_rank {
            self.parent[y_root] = x_root;
            Ok(x_root)
        } else {
            self.parent[x_root] = y_root;
            if x_rank == y_rank {
                self.rank[y_root] += 1;
            }
            Ok(y_root)
        }
    }

    /// Forces all laziness, updating every tag.
    pub fn finalize(&mut self) {
        for i in 0..self.set_size {
            DisjointSet::<T>::find_internal(&mut self.parent, i);
        }
    }
}

impl<'a> BlockChainVisitor<'a> for Clusterizer {
    type BlockItem = ();
    type TransactionItem = HashSet<Address>;
    type OutputItem = Address;
    type DoneItem = (usize, String);

    fn new() -> Self {
        Self {
            clusters: DisjointSet::new(),
        }
    }

    fn visit_block_begin(&mut self, _block: Block<'a>, _height: u64) {}

    fn visit_transaction_begin(
        &mut self,
        _block_item: &mut Self::BlockItem,
    ) -> Self::TransactionItem {
        HashSet::with_capacity(100)
    }

    fn visit_transaction_input(
        &mut self,
        txin: TransactionInput<'a>,
        _block_item: &mut Self::BlockItem,
        tx_item: &mut Self::TransactionItem,
        output_item: Option<Self::OutputItem>,
    ) {
        // Ignore coinbase
        if txin.prev_hash == &ZERO_HASH {
            return;
        }
        if let Some(address) = output_item {
            tx_item.insert(address);
        }
    }

    fn visit_transaction_output(
        &mut self,
        txout: TransactionOutput<'a>,
        _block_item: &mut (),
        _transaction_item: &mut Self::TransactionItem,
    ) -> Option<Self::OutputItem> {
        match txout.script.to_highlevel() {
            HighLevel::PayToPubkeyHash(pkh) => {
                Some(Address::from_hash160(Hash160::from_slice(pkh), 0x00))
            }
            HighLevel::PayToScriptHash(pkh) => {
                Some(Address::from_hash160(Hash160::from_slice(pkh), 0x05))
            }
            HighLevel::PayToWitnessPubkeyHash(w) | HighLevel::PayToWitnessScriptHash(w) => {
                Some(Address(w.to_address()))
            }
            _ => None,
        }
    }

    fn visit_transaction_end(
        &mut self,
        _tx: Transaction<'a>,
        _block_item: &mut Self::BlockItem,
        tx_item: Self::TransactionItem,
    ) {
        // Skip transactions with just one input
        if tx_item.len() > 1 {
            let mut tx_inputs_iter = tx_item.iter();
            let mut last_address = tx_inputs_iter.next().unwrap();
            self.clusters.make_set(last_address.to_owned());
            for address in tx_inputs_iter {
                self.clusters.make_set(address.to_owned());
                let _ = self.clusters.union(last_address, address);
                last_address = address;
            }
        }
    }

    fn done(&mut self) -> Result<(usize, String)> {
        self.clusters.finalize();

        let mut output_string = String::with_capacity(OUTPUT_STRING_CAPACITY);
        for (address, tag) in &self.clusters.map {
            output_string.push_str(&format!("{},{}\n", address, self.clusters.parent[*tag]));
        }

        info!("{} clusters generated.", self.clusters.size());
        Ok((self.clusters.size(), output_string))
    }
}
