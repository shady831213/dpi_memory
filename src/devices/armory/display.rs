use crate::devices::{FrameBuffer, PixelFormat};
use crate::memory::prelude::*;
use std::cell::{RefCell, RefMut};
use std::cmp::min;
use std::rc::Rc;

const SIMPLE_FB_PAGE_SIZE: u32 = 4096;
const SIMPLE_FB_PAGE_SIZE_SHIFT: u32 = 12;
const SIMPLE_FB_MERGE_TH: u32 = 3;
const SIMPLE_FB_REFRESH_BATCH: u32 = 32;
const SIMPLE_FB_REFRESH_BATCH_SHIFT: u32 = 5;

pub struct Fb {
    fb: RefCell<Vec<u8>>,
    width: u32,
    height: u32,
    format: PixelFormat,
    pages: u32,
    dirties: RefCell<Vec<u32>>,
}

impl Fb {
    pub fn new(width: u32, height: u32, format: PixelFormat) -> Fb {
        let size = width * height * format.size();
        let pages = (size + SIMPLE_FB_PAGE_SIZE - 1) / SIMPLE_FB_PAGE_SIZE;
        let dirties_len = (pages + SIMPLE_FB_REFRESH_BATCH - 1) >> SIMPLE_FB_REFRESH_BATCH_SHIFT;
        Fb {
            fb: RefCell::new(vec![0; size as usize]),
            width,
            height,
            format,
            pages: pages as u32,
            dirties: RefCell::new(vec![0; dirties_len as usize]),
        }
    }
    pub fn size(&self) -> u32 {
        self.pages << SIMPLE_FB_PAGE_SIZE_SHIFT
    }
    fn set_dirty(&self, offset: &u64) {
        let page = ((*offset) as u32) >> SIMPLE_FB_PAGE_SIZE_SHIFT;
        let bits = page & ((1 << SIMPLE_FB_REFRESH_BATCH_SHIFT) - 1);
        let pos = page >> SIMPLE_FB_REFRESH_BATCH_SHIFT;
        self.dirties.borrow_mut()[pos as usize] |= 1 << bits
    }
}

impl FrameBuffer for Fb {
    fn refresh<DRAW: Fn(i32, i32, u32, u32) -> Result<(), String>>(
        &self,
        draw: DRAW,
    ) -> Result<(), String> {
        let mut dirties_ref = self.dirties.borrow_mut();
        let mut page_idx: u32 = 0;
        let mut y_start: u32 = 0;
        let mut y_end: u32 = 0;
        let stride = self.stride();
        while page_idx < self.pages {
            let dirties_offset = page_idx >> SIMPLE_FB_REFRESH_BATCH_SHIFT;
            let mut dirties = dirties_ref[dirties_offset as usize];
            if dirties != 0 {
                let mut page_offset: u32 = 0;
                while dirties != 0 {
                    while (dirties >> page_offset) & 0x1 == 0 {
                        page_offset += 1;
                    }
                    dirties &= !(1 << page_offset);
                    let byte_offset = (page_idx + page_offset) << SIMPLE_FB_PAGE_SIZE_SHIFT;
                    let y_start_offset = byte_offset / stride;
                    let y_end_offset = min(
                        ((byte_offset + SIMPLE_FB_PAGE_SIZE - 1) / stride) + 1,
                        self.height,
                    );
                    if y_start == y_end {
                        y_start = y_start_offset;
                        y_end = y_end_offset;
                    } else if y_start_offset <= y_end + SIMPLE_FB_MERGE_TH {
                        y_end = y_end_offset
                    } else {
                        draw(0, y_start as i32, self.width, y_end - y_start)?;
                        y_start = y_start_offset;
                        y_end = y_end_offset;
                    }
                }
                dirties_ref[dirties_offset as usize] = 0;
            }
            page_idx += SIMPLE_FB_REFRESH_BATCH
        }

        if y_start != y_end {
            draw(0, y_start as i32, self.width, y_end - y_start)?;
        }
        Ok(())
    }
    fn data(&self) -> RefMut<'_, Vec<u8>> {
        self.fb.borrow_mut()
    }
    fn width(&self) -> u32 {
        self.width
    }
    fn height(&self) -> u32 {
        self.height
    }
    fn stride(&self) -> u32 {
        self.width * self.format.size()
    }
    fn pixel_format(&self) -> &PixelFormat {
        &self.format
    }
}

#[derive_io(Bytes, U8)]
pub struct SimpleFb(Rc<Fb>);

impl SimpleFb {
    pub fn new(fb: &Rc<Fb>) -> SimpleFb {
        SimpleFb(fb.clone())
    }
}

impl BytesAccess for SimpleFb {
    fn write(&self, addr: &u64, data: &[u8]) -> std::result::Result<usize, String> {
        self.0.set_dirty(addr);
        let offset = *addr as usize;
        self.0.fb.borrow_mut()[offset..offset + data.len()].copy_from_slice(data);
        Ok(data.len())
    }

    fn read(&self, addr: &u64, data: &mut [u8]) -> std::result::Result<usize, String> {
        let offset = *addr as usize;
        data.copy_from_slice(&self.0.fb.borrow()[offset..offset + data.len()]);
        Ok(data.len())
    }
}

impl U8Access for SimpleFb {
    fn write(&self, addr: &u64, data: u8) {
        self.0.set_dirty(addr);
        (*self.0.fb.borrow_mut())[*addr as usize] = data
    }

    fn read(&self, addr: &u64) -> u8 {
        (*self.0.fb.borrow())[*addr as usize]
    }
}
