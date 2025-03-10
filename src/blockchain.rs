extern crate dirs;

use std::collections::HashMap;

use glob::glob;
use memmap::Mmap;
use vec_map::VecMap;

use crate::block::Block;
use crate::error::ParseResult;
use crate::hash::ZERO_HASH;
use crate::preamble::*;
use crate::visitors::BlockChainVisitor;
use crate::Hash;

#[derive(PartialEq, Eq, Debug, Copy, Clone)]
struct InitIndexEntry<'a> {
    block: Option<Block<'a>>,
    child_hash: Option<Hash>,
}

pub struct BlockChain {
    maps: Vec<Mmap>,
}

impl BlockChain {
    pub unsafe fn read(blocks_dir: &str) -> BlockChain {
        let mut maps: Vec<Mmap> = Vec::new();

        for entry in glob(&format!("{}{}", blocks_dir, "/blk*.dat")).unwrap() {
            match entry {
                Ok(path) => {
                    let file = File::open(path.display().to_string()).unwrap();
                    match Mmap::map(&file) {
                        Ok(m) => {
                            maps.push(m);
                        }
                        Err(_) => {
                            break;
                        }
                    }
                }

                // if the path matched but was unreadable, thereby preventing its contents from matching
                Err(e) => println!("{:?}", e),
            }
        }

        // let blocks_dir_path = PathBuf::from(blocks_dir);
        // let mut n: usize = 0;
        // while let Ok(f) = File::open(blocks_dir_path.join(format!("blk{:05}.dat", n))) {
        //     n += 1;
        //     match Mmap::map(&f) {
        //         Ok(m) => {
        //             maps.push(m);
        //         }
        //         Err(_) => {
        //             break;
        //         }
        //     }
        // }

        BlockChain { maps }
    }

    fn walk_slice<'a, V: BlockChainVisitor<'a>>(
        &'a self,
        mut slice: &'a [u8],
        goal_prev_hash: &mut Hash,
        last_block: &mut Option<Block<'a>>,
        height: &mut u64,
        skipped: &mut HashMap<Hash, Block<'a>>,
        output_items: &mut HashMap<Hash, VecMap<V::OutputItem>>,
        visitor: &mut V,
    ) -> ParseResult<()> {
        while !slice.is_empty() {
            if skipped.contains_key(goal_prev_hash) {
                last_block.unwrap().walk(visitor, *height, output_items)?;
                debug!(
                    "(rewind - pre-step) Block {} - {} -> {}",
                    height,
                    last_block.unwrap().header().prev_hash(),
                    last_block.unwrap().header().cur_hash()
                );
                *height += 1;
                while let Some(block) = skipped.remove(goal_prev_hash) {
                    block.walk(visitor, *height, output_items)?;
                    debug!(
                        "(rewind) Block {} - {} -> {}",
                        height,
                        block.header().prev_hash(),
                        block.header().cur_hash()
                    );
                    *height += 1;
                    *goal_prev_hash = block.header().cur_hash();
                    *last_block = None;
                }
            }

            let block = match Block::read(&mut slice)? {
                Some(block) => block,
                None => {
                    assert_eq!(slice.len(), 0);
                    break;
                }
            };

            debug!("Block candidate for height {} - goal_prev_hash = {}, prev_hash = {}, cur_hash = {}", height, goal_prev_hash.to_string(), block.header().prev_hash(), block.header().cur_hash());

            if block.header().prev_hash() != goal_prev_hash {
                skipped.insert(*block.header().prev_hash(), block);

                if last_block.is_some()
                    && block.header().prev_hash() == last_block.unwrap().header().prev_hash()
                {
                    debug!(
                        "Chain split detected: {} <-> {}. Detecting main chain and orphan.",
                        last_block.unwrap().header().cur_hash(),
                        block.header().cur_hash()
                    );

                    let first_orphan = last_block.unwrap();
                    let second_orphan = block;

                    loop {
                        let block = match Block::read(&mut slice)? {
                            Some(block) => block,
                            None => {
                                assert_eq!(slice.len(), 0);
                                break;
                            }
                        };
                        skipped.insert(*block.header().prev_hash(), block);
                        if block.header().prev_hash() == &first_orphan.header().cur_hash() {
                            // First wins
                            debug!(
                                "Chain split: {} is on the main chain!",
                                first_orphan.header().cur_hash()
                            );
                            break;
                        }
                        if block.header().prev_hash() == &second_orphan.header().cur_hash() {
                            // Second wins
                            debug!(
                                "Chain split: {} is on the main chain!",
                                second_orphan.header().cur_hash()
                            );
                            *goal_prev_hash = second_orphan.header().cur_hash();
                            *last_block = Some(second_orphan);
                            break;
                        }
                    }
                }
                continue;
            }

            if let Some(last_block) = *last_block {
                last_block.walk(visitor, *height, output_items)?;
                debug!(
                    "(last_block) Block {} - {} -> {}",
                    height,
                    last_block.header().prev_hash(),
                    last_block.header().cur_hash()
                );
                *height += 1;
            }

            *goal_prev_hash = block.header().cur_hash();
            *last_block = Some(block);
        }

        Ok(())
    }

    pub fn walk<'a, V: BlockChainVisitor<'a>>(
        &'a self,
        visitor: &mut V,
    ) -> ParseResult<(u64, Hash, HashMap<Hash, VecMap<V::OutputItem>>)> {
        let mut skipped: HashMap<Hash, Block> = Default::default();
        let mut output_items: HashMap<Hash, VecMap<V::OutputItem>> = Default::default();
        let mut goal_prev_hash: Hash = ZERO_HASH;
        let mut last_block: Option<Block> = None;
        let mut height = 0;

        for (n, map) in self.maps.iter().enumerate() {
            info!(
                "Parsing the blockchain: block file {}/{}...",
                n + 1,
                self.maps.len()
            );
            self.walk_slice(
                map,
                &mut goal_prev_hash,
                &mut last_block,
                &mut height,
                &mut skipped,
                &mut output_items,
                visitor,
            )?;
        }

        Ok((height, goal_prev_hash, output_items))
    }
}
