#![feature(test)]
extern crate test;

use test::Bencher;

extern crate terminus_spaceport;

use terminus_spaceport::memory::*;
use rand::Rng;
use std::ops::Deref;

const MAX_RND:usize = 1000000;
#[bench]
fn bench_model_access(b: &mut Bencher) {
    let region = Heap::global().alloc(0x1_0000_0000, 1);
    let mut rng = rand::thread_rng();
    let mut addrs = vec![];
    for _ in 0 .. MAX_RND {
        addrs.push(rng.gen::<u64>() % 0x1_0000_0000)
    }
    let mut i = 0;
    let mut get_addr = || {
        let data = addrs.get(i).unwrap();
        if i == MAX_RND - 1 {
            i = 0
        } else {
            i = i + 1
        }
        *data
    };
    b.iter(|| {
        U8Access::write(region.deref(), get_addr(), 0xaa);
        U8Access::read(region.deref(), get_addr());
    });
}