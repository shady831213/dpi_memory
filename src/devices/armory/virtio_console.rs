use crate::devices::TERM;
use crate::irq::IrqVecSender;
use crate::memory::prelude::*;
use crate::memory::region::Region;
use crate::virtio::{Device, DeviceAccess, MMIODevice, Queue, QueueClient, QueueSetting, Result};
use std::cmp::min;
use std::io::{ErrorKind, Read, Write};
use std::ops::Deref;
use std::rc::Rc;

struct VirtIOConsoleInputQueue {}

impl VirtIOConsoleInputQueue {
    fn new() -> VirtIOConsoleInputQueue {
        VirtIOConsoleInputQueue {}
    }
}

impl QueueClient for VirtIOConsoleInputQueue {
    fn receive(&self, _: &Queue, _: u16) -> Result<bool> {
        Ok(false)
    }
}

struct VirtIOConsoleOutputQueue {
    memory: Rc<Region>,
    irq_sender: IrqVecSender,
}

impl VirtIOConsoleOutputQueue {
    fn new(memory: &Rc<Region>, irq_sender: IrqVecSender) -> VirtIOConsoleOutputQueue {
        VirtIOConsoleOutputQueue {
            memory: memory.clone(),
            irq_sender,
        }
    }
}

impl QueueClient for VirtIOConsoleOutputQueue {
    fn receive(&self, queue: &Queue, desc_head: u16) -> Result<bool> {
        let desc = queue.get_desc(desc_head)?;
        if desc.len > 0 {
            let mut buffer: Vec<u8> = vec![0; desc.len as usize];
            BytesAccess::read(self.memory.deref(), &desc.addr, &mut buffer).unwrap();
            let stdout = TERM.stdout();
            let mut handle = stdout.lock();
            handle.write(&buffer).unwrap();
            handle.flush().unwrap();
        }
        queue.set_used(desc_head, desc.len)?;
        queue.update_last_avail();
        self.irq_sender.send().unwrap();
        Ok(true)
    }
}

pub struct VirtIOConsoleDevice {
    virtio_device: Device,
}

impl VirtIOConsoleDevice {
    pub fn new(memory: &Rc<Region>, irq_sender: IrqVecSender) -> VirtIOConsoleDevice {
        let mut virtio_device = Device::new(memory, irq_sender, 1, 3, 0, 1);
        virtio_device.get_irq_vec().set_enable_uncheck(0, true);
        let input_queue = {
            let input = VirtIOConsoleInputQueue::new();
            Queue::new(&memory, QueueSetting { max_queue_size: 1 }, input)
        };
        let output_queue = {
            let output = VirtIOConsoleOutputQueue::new(
                memory,
                virtio_device.get_irq_vec().sender(0).unwrap(),
            );
            Queue::new(&memory, QueueSetting { max_queue_size: 1 }, output)
        };
        virtio_device.add_queue(input_queue);
        virtio_device.add_queue(output_queue);
        VirtIOConsoleDevice { virtio_device }
    }
    pub fn console_read(&self) {
        let input_queue = self.virtio_device.get_queue(0);
        if !input_queue.get_ready() {
            return;
        }
        if let Some(desc_head) = input_queue.avail_iter().unwrap().last() {
            let desc = input_queue.get_desc(desc_head).unwrap();
            let len = min(desc.len as usize, 128);
            let mut buffer: Vec<u8> = vec![0; len];
            let ret = match TERM.stdin().lock().read(&mut buffer) {
                Ok(l) => l,
                Err(e) if e.kind() == ErrorKind::WouldBlock => 0,
                Err(e) => panic!("{:?}", e),
            };
            if ret > 0 {
                BytesAccess::write(
                    self.virtio_device.memory().deref(),
                    &desc.addr,
                    &buffer[..ret],
                )
                .unwrap();
                input_queue.set_used(desc_head, ret as u32).unwrap();
                input_queue.update_last_avail();
                self.virtio_device
                    .get_irq_vec()
                    .sender(0)
                    .unwrap()
                    .send()
                    .unwrap();
            }
        }
    }
}

#[derive_io(Bytes)]
pub struct VirtIOConsole(Rc<VirtIOConsoleDevice>);

impl VirtIOConsole {
    pub fn new(d: &Rc<VirtIOConsoleDevice>) -> VirtIOConsole {
        VirtIOConsole(d.clone())
    }
}

impl DeviceAccess for VirtIOConsole {
    fn device(&self) -> &Device {
        &self.0.virtio_device
    }
}

impl MMIODevice for VirtIOConsole {}

impl BytesAccess for VirtIOConsole {
    fn write(&self, addr: &u64, data: &[u8]) -> std::result::Result<usize, String> {
        self.write_bytes(addr, data);
        Ok(0)
    }

    fn read(&self, addr: &u64, data: &mut [u8]) -> std::result::Result<usize, String> {
        self.read_bytes(addr, data);
        Ok(0)
    }
}
