use std::fmt::write;

use crate::{config::SimulatorConfig, inst::{mem::{Direction, MemInst}}, sim::unit::buffer::{BufferEvent, BufferEventResult, BufferPair}};



pub struct LoadStoreUnit {
    latency: u32,
    max_access_width: u32,
    read_port_buffer : Vec<BufferPair>,
    write_port_buffer : Vec<BufferPair>,
    read_port_used : Vec<Option<MemoryPortEventGenerator>>,
    write_port_used : Vec<Option<MemoryPortEventGenerator>>
}

type PortNumberIdType = usize;
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MemoryUnitKeyType {
    Load(PortNumberIdType),
    Store(PortNumberIdType)
}

pub struct MemoryPortEventGenerator {
    index : usize,
    bytes_per_cycle : u32,
    raw_inst : MemInst,
    total_bytes : u32,
    current_pos : u32,
}

impl MemoryPortEventGenerator {
    pub fn new(index : usize, raw_inst : MemInst) -> Self {
        let config = SimulatorConfig::get_global_config().unwrap();
        Self {
            index,
            bytes_per_cycle: config.get_max_access_width(),
            raw_inst,
            total_bytes: raw_inst.get_total_bytes(),
            current_pos : 0,
        }
    }
}

impl LoadStoreUnit {
    pub fn new(latency: u32, max_access_width: u32) -> LoadStoreUnit {
        let read_port_count = SimulatorConfig::get_global_config().unwrap().get_memory_read_ports_limit();
        let write_port_count = SimulatorConfig::get_global_config().unwrap().get_memory_write_ports_limit();

        LoadStoreUnit {
            latency,
            max_access_width,
            read_port_buffer: Vec::with_capacity(read_port_count),
            write_port_buffer: Vec::with_capacity(write_port_count),
            read_port_used : Vec::with_capacity(read_port_count),
            write_port_used : Vec::with_capacity(write_port_count)
        }
    }

    pub fn new_from_config(config: &crate::config::LoadStoreUnit) -> LoadStoreUnit {
        LoadStoreUnit::new(config.latency, config.max_access_width)
    }

    pub fn handle_buffer_event(&mut self, key : MemoryUnitKeyType, event : BufferEvent) -> BufferEventResult {
        let res = match key {
            MemoryUnitKeyType::Load(i) => {
                self.read_port_buffer[i].handle_buffer_event(event)
            },
            MemoryUnitKeyType::Store(i) => {
                self.write_port_buffer[i].handle_buffer_event(event)
            }
        };

        match res {
            Ok(r) => r,
            Err(err) => panic!("Buffer event handling error: {}", err)
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        true
    }
    pub fn handle_event_queue(&mut self) -> anyhow::Result<()> {
        // 第一步： 读口和写口分别传递数据
        let mut finish_read_port : Vec<usize> = Vec::new();
        for i in 0..self.read_port_used.len() {
            if let Some(ref mut port) = self.read_port_used[i] {
                let update_bytes = self.read_port_buffer[i].get_memory_input_current_bytes()?.min(port.bytes_per_cycle);
                port.current_pos += update_bytes;
                self.read_port_buffer[i].increase_result(update_bytes)?;

                if port.current_pos == port.total_bytes {
                    finish_read_port.push(i);
                }
            }
        }
        // TODO: 之后统一处理下清空的逻辑
        finish_read_port.into_iter().for_each(|i| self.read_port_used[i] = None);

        let mut finish_write_port : Vec<usize> = Vec::new();
        for i in 0..self.write_port_used.len() {
            if let Some(ref mut port) = self.write_port_used[i] {
                let update_bytes = self.write_port_buffer[i].get_register_input_current_bytes()?.min(port.bytes_per_cycle);
                port.current_pos += update_bytes;
                self.write_port_buffer[i].increase_result(update_bytes)?;

                if port.current_pos == port.total_bytes {
                    finish_write_port.push(i);
                }

            }
        }
        finish_write_port.into_iter().for_each(|i| self.write_port_used[i] = None);
        

        Ok(())
    }

    pub fn has_free_port(&self, dir : Direction) -> bool {
        match dir {
            Direction::Read => {
                self.read_port_used.iter().position(|used| used.is_none()).is_some()
            },
            Direction::Write => {
                self.write_port_used.iter().position(|used| used.is_none()).is_some()
            }
        }
    }

    fn set_port_event(&mut self, mem_inst : MemInst) -> anyhow::Result<usize> {
        match mem_inst.dir {
            Direction::Read => {
                let port_index = self.read_port_used.iter().position(|used| used.is_none()).unwrap();
                self.read_port_used[port_index] = Some(MemoryPortEventGenerator::new(port_index, mem_inst));
                Ok(port_index)
            },
            Direction::Write => {
                let port_index = self.write_port_used.iter().position(|used| used.is_none()).unwrap();
                self.write_port_used[port_index] = Some(MemoryPortEventGenerator::new(port_index, mem_inst));
                Ok(port_index)
            }
        }
    }
    pub fn issue(&mut self, mem_inst : MemInst) -> anyhow::Result<()> {
        let index = self.set_port_event(mem_inst)?;
        use crate::sim::unit::buffer::Resource;
        match mem_inst.dir {
            
            Direction::Read => {
                self.read_port_buffer[index].set_input(vec![Resource::new(crate::sim::unit::buffer::ResourceType::Memory, mem_inst.get_total_bytes())])?;

                self.read_port_buffer[index].set_output(Resource::new(crate::sim::unit::buffer::ResourceType::Register(mem_inst.reg), mem_inst.get_total_bytes()));
            },
            Direction::Write => {
                self.write_port_buffer[index].set_input(vec![Resource::new(crate::sim::unit::buffer::ResourceType::Register(mem_inst.reg), mem_inst.get_total_bytes())])?;
                self.write_port_buffer[index].set_output(Resource::new(crate::sim::unit::buffer::ResourceType::Memory, mem_inst.get_total_bytes()));
            }
        }
        Ok(())
    }
}