use crate::irq::IrqVecSender;
use crate::memory::prelude::*;
use crate::memory::region::{Region, GHEAP};
use crate::virtio::{
    DescMeta, Device, DeviceAccess, Error, MMIODevice, Queue, QueueClient, QueueSetting, Result,
};
use std::fs;
use std::fs::{File, OpenOptions};
use std::ops::Deref;
use std::os::unix::prelude::FileExt;
use std::rc::Rc;

const VIRTIO_BLK_T_IN: u32 = 0;
const VIRTIO_BLK_T_OUT: u32 = 1;
//SECTOR_SIZE= 512
const VIRTIO_BLK_SECTOR_SHIFT: u64 = 9;

const VIRTIO_BLK_S_OK: u8 = 0;
const VIRTIO_BLK_S_IOERR: u8 = 1;

#[derive(Default, Debug)]
#[repr(C)]
struct VirtIOBlkHeader {
    ty: u32,
    ioprio: u32,
    sector_num: u64,
}

pub enum VirtIOBlkConfig {
    RO,
    RW,
    SNAPSHOT,
}

impl VirtIOBlkConfig {
    pub fn new(val: &str) -> VirtIOBlkConfig {
        match val {
            "ro" => VirtIOBlkConfig::RO,
            "rw" => VirtIOBlkConfig::RW,
            _ => VirtIOBlkConfig::SNAPSHOT,
        }
    }
}

struct VirtIOBlkDiskSnapshot {
    snapshot: Rc<Region>,
}

impl VirtIOBlkDiskSnapshot {
    fn new(snapshot: &Rc<Region>) -> VirtIOBlkDiskSnapshot {
        VirtIOBlkDiskSnapshot {
            snapshot: snapshot.clone(),
        }
    }
}

impl BytesAccess for VirtIOBlkDiskSnapshot {
    fn write(&self, addr: &u64, data: &[u8]) -> std::result::Result<usize, String> {
        if *addr + data.len() as u64 > self.snapshot.info.size {
            Err("out of range!".to_string())
        } else {
            BytesAccess::write(self.snapshot.deref(), addr, data)
        }
    }

    fn read(&self, addr: &u64, data: &mut [u8]) -> std::result::Result<usize, String> {
        if *addr + data.len() as u64 > self.snapshot.info.size {
            Err("out of range!".to_string())
        } else {
            BytesAccess::read(self.snapshot.deref(), addr, data)
        }
    }
}

struct VirtIOBlkFile {
    fp: Rc<File>,
}

impl VirtIOBlkFile {
    fn new(fp: &Rc<File>) -> VirtIOBlkFile {
        VirtIOBlkFile { fp: fp.clone() }
    }
}

impl BytesAccess for VirtIOBlkFile {
    fn write(&self, addr: &u64, data: &[u8]) -> std::result::Result<usize, String> {
        if self.fp.write_all_at(data, *addr).is_err() {
            Err("write err".to_string())
        } else if self.fp.sync_all().is_err() {
            Err("sync err".to_string())
        } else {
            Ok(data.len())
        }
    }

    fn read(&self, addr: &u64, data: &mut [u8]) -> std::result::Result<usize, String> {
        if self.fp.sync_all().is_err() {
            Err("sync err".to_string())
        } else if self.fp.read_exact_at(data, *addr).is_err() {
            Err("read err".to_string())
        } else {
            Ok(data.len())
        }
    }
}

struct VirtIOBlkQueue<T: BytesAccess> {
    memory: Rc<Region>,
    disk: T,
    irq_sender: IrqVecSender,
}

impl<T: BytesAccess> VirtIOBlkQueue<T> {
    fn new(memory: &Rc<Region>, disk: T, irq_sender: IrqVecSender) -> VirtIOBlkQueue<T> {
        VirtIOBlkQueue {
            memory: memory.clone(),
            disk,
            irq_sender,
        }
    }
}

impl<T: BytesAccess> QueueClient for VirtIOBlkQueue<T> {
    fn receive(&self, queue: &Queue, desc_head: u16) -> Result<bool> {
        let mut read_descs: Vec<DescMeta> = vec![];
        let mut write_descs: Vec<DescMeta> = vec![];
        let mut write_buffer: Vec<u8> = vec![];
        let mut read_buffer: Vec<u8> = vec![];
        let (read_len, write_len) = queue.extract(
            desc_head,
            &mut read_buffer,
            &mut write_buffer,
            &mut read_descs,
            &mut write_descs,
            true,
            true,
        )?;
        let mut header = VirtIOBlkHeader::default();
        let header_size = std::mem::size_of::<VirtIOBlkHeader>();
        if write_len as usize >= header_size {
            unsafe {
                std::slice::from_raw_parts_mut(
                    (&mut header as *mut VirtIOBlkHeader) as *mut u8,
                    header_size,
                )
                .copy_from_slice(&write_buffer[..header_size])
            }
        } else {
            return Err(Error::ClientError("invalid block header!".to_string()));
        }

        let disk_offset = header.sector_num << VIRTIO_BLK_SECTOR_SHIFT;

        match header.ty {
            VIRTIO_BLK_T_IN => {
                if BytesAccess::read(
                    &self.disk,
                    &disk_offset,
                    &mut read_buffer[..read_len as usize - 1],
                )
                .is_ok()
                {
                    read_buffer[read_len as usize - 1] = VIRTIO_BLK_S_OK;
                } else {
                    read_buffer[read_len as usize - 1] = VIRTIO_BLK_S_IOERR;
                }
                queue.copy_to(&read_descs, &read_buffer)?;
                queue.set_used(desc_head, read_len as u32)?;
            }
            VIRTIO_BLK_T_OUT => {
                if BytesAccess::write(&self.disk, &disk_offset, &write_buffer[header_size..])
                    .is_ok()
                {
                    U8Access::write(
                        self.memory.deref(),
                        &read_descs.first().unwrap().addr,
                        VIRTIO_BLK_S_OK,
                    );
                } else {
                    U8Access::write(
                        self.memory.deref(),
                        &read_descs.first().unwrap().addr,
                        VIRTIO_BLK_S_IOERR,
                    );
                }
                queue.set_used(desc_head, 1)?;
            }
            _ => {
                return Err(Error::ClientError(format!(
                    "invalid block ty {:#x}!",
                    header.ty
                )))
            }
        }
        queue.update_last_avail();
        self.irq_sender.send().unwrap();
        Ok(true)
    }
}

#[derive_io(Bytes)]
pub struct VirtIOBlk {
    virtio_device: Device,
    num_sectors: u64,
}

impl VirtIOBlk {
    pub fn new(
        memory: &Rc<Region>,
        irq_sender: IrqVecSender,
        num_queues: usize,
        file_name: &str,
        config: VirtIOBlkConfig,
    ) -> VirtIOBlk {
        assert!(num_queues > 0);
        let mut virtio_device = Device::new(memory, irq_sender, 1, 2, 0, 0);
        virtio_device.get_irq_vec().set_enable_uncheck(0, true);
        let len = match config {
            VirtIOBlkConfig::RO => {
                let file = Rc::new(
                    OpenOptions::new()
                        .read(true)
                        .open(file_name)
                        .expect(&format!("can not open {}!", file_name)),
                );
                for _ in 0..num_queues {
                    // virtio_device.add_queue(Queue::new(&memory, QueueSetting { max_queue_size: 16 }, VirtIOBlkQueue::new(memory, VirtIOBlkDiskSnapshot::new(&snapshot), virtio_device.get_irq_vec().sender(0).unwrap())));
                    virtio_device.add_queue(Queue::new(
                        &memory,
                        QueueSetting { max_queue_size: 16 },
                        VirtIOBlkQueue::new(
                            memory,
                            VirtIOBlkFile::new(&file),
                            virtio_device.get_irq_vec().sender(0).unwrap(),
                        ),
                    ));
                }
                file.metadata().unwrap().len()
            }
            VirtIOBlkConfig::RW => {
                let file = Rc::new(
                    OpenOptions::new()
                        .read(true)
                        .write(true)
                        .create(true)
                        .open(file_name)
                        .expect(&format!("can not open {}!", file_name)),
                );
                for _ in 0..num_queues {
                    // virtio_device.add_queue(Queue::new(&memory, QueueSetting { max_queue_size: 16 }, VirtIOBlkQueue::new(memory, VirtIOBlkDiskSnapshot::new(&snapshot), virtio_device.get_irq_vec().sender(0).unwrap())));
                    virtio_device.add_queue(Queue::new(
                        &memory,
                        QueueSetting { max_queue_size: 16 },
                        VirtIOBlkQueue::new(
                            memory,
                            VirtIOBlkFile::new(&file),
                            virtio_device.get_irq_vec().sender(0).unwrap(),
                        ),
                    ));
                }
                file.metadata().unwrap().len()
            }
            VirtIOBlkConfig::SNAPSHOT => {
                let content = fs::read(file_name).unwrap().into_boxed_slice();
                let snapshot = Region::remap(0, &GHEAP.alloc(content.len() as u64, 1).unwrap());
                BytesAccess::write(snapshot.deref(), &0, &content).unwrap();
                for _ in 0..num_queues {
                    virtio_device.add_queue(Queue::new(
                        &memory,
                        QueueSetting { max_queue_size: 16 },
                        VirtIOBlkQueue::new(
                            memory,
                            VirtIOBlkDiskSnapshot::new(&snapshot),
                            virtio_device.get_irq_vec().sender(0).unwrap(),
                        ),
                    ));
                }
                content.len() as u64
            }
        };
        VirtIOBlk {
            virtio_device,
            num_sectors: len >> VIRTIO_BLK_SECTOR_SHIFT,
        }
    }
}

impl DeviceAccess for VirtIOBlk {
    fn device(&self) -> &Device {
        &self.virtio_device
    }
    fn config(&self, offset: u64, data: &mut [u8]) {
        let len = data.len();
        let off = offset as usize;
        if off < 8 && (off + len) <= 8 {
            data.copy_from_slice(&self.num_sectors.to_le_bytes()[off..off + len])
        }
    }
}

impl MMIODevice for VirtIOBlk {}

impl BytesAccess for VirtIOBlk {
    fn write(&self, addr: &u64, data: &[u8]) -> std::result::Result<usize, String> {
        self.write_bytes(addr, data);
        Ok(0)
    }

    fn read(&self, addr: &u64, data: &mut [u8]) -> std::result::Result<usize, String> {
        self.read_bytes(addr, data);
        Ok(0)
    }
}
