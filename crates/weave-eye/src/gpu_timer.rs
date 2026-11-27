use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::graph::RenderGraph;

pub(crate) const RING: usize = 3;

pub(crate) struct GpuTimer {
    set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    readback: [wgpu::Buffer; RING],
    pending: [Option<Arc<AtomicBool>>; RING],
    period_ns: f64,
    passes: usize,
    frame: usize,
    active: Option<usize>,
    blit_used: [bool; RING],
}

impl GpuTimer {
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue, passes: usize) -> Self {
        let count = u32::try_from(passes * 2 + 2).unwrap_or(u32::MAX);
        let bytes = u64::from(count) * 8;
        let set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("pass timestamps"),
            ty: wgpu::QueryType::Timestamp,
            count,
        });
        let resolve = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("timestamp resolve"),
            size: bytes,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = std::array::from_fn(|_| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("timestamp readback"),
                size: bytes,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        Self {
            set,
            resolve,
            readback,
            pending: [None, None, None],
            period_ns: f64::from(queue.get_timestamp_period()),
            passes,
            frame: 0,
            active: None,
            blit_used: [false; RING],
        }
    }

    pub(crate) fn query_set(&self) -> &wgpu::QuerySet {
        &self.set
    }

    pub(crate) fn begin_frame(&mut self, device: &wgpu::Device, graph: &mut RenderGraph) -> bool {
        let _ = device.poll(wgpu::PollType::Poll);
        for slot in 0..RING {
            let ready = self.pending[slot]
                .as_ref()
                .is_some_and(|flag| flag.load(Ordering::Acquire));
            if ready {
                self.read(slot, graph);
                self.pending[slot] = None;
            }
        }
        let slot = self.frame % RING;
        self.frame += 1;
        if self.pending[slot].is_some() {
            self.active = None;
            return false;
        }
        self.active = Some(slot);
        true
    }

    pub(crate) fn finish_frame(&mut self, encoder: &mut wgpu::CommandEncoder, blit: bool) {
        let Some(slot) = self.active else {
            return;
        };
        self.blit_used[slot] = blit;
        let count = u32::try_from(self.passes * 2 + 2).unwrap_or(u32::MAX);
        encoder.resolve_query_set(&self.set, 0..count, &self.resolve, 0);
        encoder.copy_buffer_to_buffer(
            &self.resolve,
            0,
            &self.readback[slot],
            0,
            u64::from(count) * 8,
        );
    }

    pub(crate) fn map_after_submit(&mut self) {
        let Some(slot) = self.active.take() else {
            return;
        };
        let flag = Arc::new(AtomicBool::new(false));
        let done = Arc::clone(&flag);
        self.readback[slot]
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                if result.is_ok() {
                    done.store(true, Ordering::Release);
                }
            });
        self.pending[slot] = Some(flag);
    }

    fn read(&mut self, slot: usize, graph: &mut RenderGraph) {
        let buffer = &self.readback[slot];
        if let Ok(view) = buffer.slice(..).get_mapped_range() {
            let data: &[u8] = &view;
            let mut first = u64::MAX;
            let mut last = 0u64;
            for pass in 0..self.passes {
                let begin = u64_at(data, pass * 16);
                let end = u64_at(data, pass * 16 + 8);
                if end >= begin {
                    let nanos = (end - begin) as f64 * self.period_ns;
                    graph.record_gpu(pass, Duration::from_nanos(nanos as u64));
                    if end > 0 {
                        first = first.min(begin);
                        last = last.max(end);
                    }
                }
            }
            if self.blit_used[slot] {
                let begin = u64_at(data, self.passes * 16);
                let end = u64_at(data, self.passes * 16 + 8);
                if end >= begin && end > 0 {
                    first = first.min(begin);
                    last = last.max(end);
                }
            }
            if last > first {
                let nanos = (last - first) as f64 * self.period_ns;
                graph.record_gpu_frame(Duration::from_nanos(nanos as u64));
            }
        }
        buffer.unmap();
    }
}

fn u64_at(data: &[u8], offset: usize) -> u64 {
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&data[offset..offset + 8]);
    u64::from_le_bytes(bytes)
}
