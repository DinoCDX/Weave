use std::fmt::Write as _;
use std::time::{Duration, Instant};

use crate::renderer::DrawResources;

pub struct PassContext<'a> {
    pub encoder: &'a mut wgpu::CommandEncoder,
    pub color: &'a wgpu::TextureView,
    pub depth: &'a wgpu::TextureView,
    pub(crate) draws: &'a DrawResources,
    pub(crate) timestamps: Option<&'a wgpu::QuerySet>,
    pub(crate) slot: usize,
}

impl<'a> PassContext<'a> {
    pub(crate) fn timestamp_writes(&self) -> Option<wgpu::RenderPassTimestampWrites<'a>> {
        self.timestamps.map(|set| wgpu::RenderPassTimestampWrites {
            query_set: set,
            beginning_of_pass_write_index: Some(u32::try_from(self.slot * 2).unwrap_or(0)),
            end_of_pass_write_index: Some(u32::try_from(self.slot * 2 + 1).unwrap_or(0)),
        })
    }
}

pub trait Pass {
    fn name(&self) -> &'static str;

    fn reads(&self) -> &'static [&'static str];

    fn writes(&self) -> &'static [&'static str];

    fn record(&mut self, cx: &mut PassContext<'_>);
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GraphError {
    DuplicatePass(&'static str),
    UnwrittenResource {
        pass: &'static str,
        resource: &'static str,
    },
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicatePass(name) => write!(f, "two passes are named {name:?}"),
            Self::UnwrittenResource { pass, resource } => write!(
                f,
                "pass {pass:?} reads {resource:?}, which no earlier pass writes"
            ),
        }
    }
}

impl std::error::Error for GraphError {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Timing {
    pub last: Duration,
    pub total: Duration,
    pub calls: u64,
}

impl Timing {
    pub fn average(&self) -> Duration {
        match self.calls {
            0 => Duration::ZERO,
            n => self.total / u32::try_from(n).unwrap_or(u32::MAX),
        }
    }

    fn add(&mut self, elapsed: Duration) {
        self.last = elapsed;
        self.total += elapsed;
        self.calls += 1;
    }
}

struct Slot {
    pass: Box<dyn Pass>,
    timing: Timing,
    gpu: Timing,
}

pub struct RenderGraph {
    slots: Vec<Slot>,
    gpu_frame: Option<Duration>,
}

impl RenderGraph {
    pub fn new(passes: Vec<Box<dyn Pass>>) -> Result<Self, GraphError> {
        let mut names: Vec<&'static str> = Vec::new();
        let mut written: Vec<&'static str> = Vec::new();
        for pass in &passes {
            let name = pass.name();
            if names.contains(&name) {
                return Err(GraphError::DuplicatePass(name));
            }
            names.push(name);
            for &resource in pass.reads() {
                if !written.contains(&resource) {
                    return Err(GraphError::UnwrittenResource {
                        pass: name,
                        resource,
                    });
                }
            }
            for &resource in pass.writes() {
                if !written.contains(&resource) {
                    written.push(resource);
                }
            }
        }
        Ok(Self {
            gpu_frame: None,
            slots: passes
                .into_iter()
                .map(|pass| Slot {
                    pass,
                    timing: Timing::default(),
                    gpu: Timing::default(),
                })
                .collect(),
        })
    }

    pub fn pass_names(&self) -> Vec<&'static str> {
        self.slots.iter().map(|slot| slot.pass.name()).collect()
    }

    pub fn timing(&self, name: &str) -> Option<Timing> {
        self.slots
            .iter()
            .find(|slot| slot.pass.name() == name)
            .map(|slot| slot.timing)
    }

    pub fn gpu_timing(&self, name: &str) -> Option<Timing> {
        self.slots
            .iter()
            .find(|slot| slot.pass.name() == name)
            .map(|slot| slot.gpu)
    }

    pub(crate) fn record_gpu_frame(&mut self, elapsed: Duration) {
        self.gpu_frame = Some(elapsed);
    }

    pub(crate) fn take_gpu_frame(&mut self) -> Option<Duration> {
        self.gpu_frame.take()
    }

    pub(crate) fn record_gpu(&mut self, index: usize, elapsed: Duration) {
        if let Some(slot) = self.slots.get_mut(index) {
            slot.gpu.add(elapsed);
        }
    }

    pub(crate) fn slot_count(&self) -> usize {
        self.slots.len()
    }

    pub(crate) fn execute(&mut self, cx: &mut PassContext<'_>) {
        for (index, slot) in self.slots.iter_mut().enumerate() {
            cx.slot = index;
            let start = Instant::now();
            slot.pass.record(cx);
            slot.timing.add(start.elapsed());
        }
    }

    pub fn report(&self) -> String {
        let mut out = String::from(
            "pass             cpu last (us)   cpu average (us)   gpu average (us)   calls\n",
        );
        for slot in &self.slots {
            let gpu = if slot.gpu.calls == 0 {
                "not measured".to_owned()
            } else {
                format!("{:.1}", slot.gpu.average().as_secs_f64() * 1e6)
            };
            let _ = writeln!(
                out,
                "{:<16} {:>13.1} {:>18.1} {:>18} {:>7}",
                slot.pass.name(),
                slot.timing.last.as_secs_f64() * 1e6,
                slot.timing.average().as_secs_f64() * 1e6,
                gpu,
                slot.timing.calls
            );
        }
        out
    }
}

const CLEAR_COLOR: wgpu::Color = wgpu::Color {
    r: 0.05,
    g: 0.05,
    b: 0.08,
    a: 1.0,
};

pub(crate) struct ClearPass;

impl Pass for ClearPass {
    fn name(&self) -> &'static str {
        "clear"
    }

    fn reads(&self) -> &'static [&'static str] {
        &[]
    }

    fn writes(&self) -> &'static [&'static str] {
        &["color", "depth"]
    }

    fn record(&mut self, cx: &mut PassContext<'_>) {
        let timestamp_writes = cx.timestamp_writes();
        let _pass = cx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: cx.color,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(CLEAR_COLOR),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: cx.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
}

pub(crate) struct OpaquePass;

impl Pass for OpaquePass {
    fn name(&self) -> &'static str {
        "opaque"
    }

    fn reads(&self) -> &'static [&'static str] {
        &[
            "color", "depth", "depth2", "shadow", "cull", "cull2", "ao_blur",
        ]
    }

    fn writes(&self) -> &'static [&'static str] {
        &["color"]
    }

    fn record(&mut self, cx: &mut PassContext<'_>) {
        let draws = cx.draws;
        let timestamp_writes = cx.timestamp_writes();
        let mut pass = cx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("opaque"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: cx.color,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: cx.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if draws.batches.is_empty() {
            return;
        }
        pass.set_pipeline(if draws.use_direct {
            &draws.direct
        } else {
            &draws.pipeline
        });
        pass.set_bind_group(0, &draws.bind_group, &[]);
        draw_indirect(&mut pass, draws, &draws.camera_compact, 0);
        if draws.occlusion_active {
            draw_indirect(&mut pass, draws, &draws.camera_compact2, 1);
        }
    }
}

pub(crate) struct TransparentPass;

impl Pass for TransparentPass {
    fn name(&self) -> &'static str {
        "transparent"
    }

    fn reads(&self) -> &'static [&'static str] {
        &["color", "depth"]
    }

    fn writes(&self) -> &'static [&'static str] {
        &["color"]
    }

    fn record(&mut self, cx: &mut PassContext<'_>) {
        empty_pass(cx, "transparent");
    }
}

pub(crate) struct PostPass;

impl Pass for PostPass {
    fn name(&self) -> &'static str {
        "post"
    }

    fn reads(&self) -> &'static [&'static str] {
        &["color"]
    }

    fn writes(&self) -> &'static [&'static str] {
        &["color"]
    }

    fn record(&mut self, cx: &mut PassContext<'_>) {
        empty_pass(cx, "post");
    }
}

fn empty_pass(cx: &mut PassContext<'_>, label: &'static str) {
    let timestamp_writes = cx.timestamp_writes();
    let _pass = cx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: cx.color,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes,
        occlusion_query_set: None,
        multiview_mask: None,
    });
}

pub(crate) struct CullPass;

impl Pass for CullPass {
    fn name(&self) -> &'static str {
        "cull"
    }

    fn reads(&self) -> &'static [&'static str] {
        &[]
    }

    fn writes(&self) -> &'static [&'static str] {
        &["cull"]
    }

    fn record(&mut self, cx: &mut PassContext<'_>) {
        let draws = cx.draws;
        if draws.batches.is_empty() {
            return;
        }
        let mut pass = cx.encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("cull"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&draws.cull_pipeline);
        for (index, groups) in draws.groups.iter().enumerate() {
            if *groups > 0 {
                pass.set_bind_group(0, &draws.binds[index][0], &[]);
                pass.dispatch_workgroups(*groups, 1, 1);
            }
        }
        if draws.shadows_enabled && !draws.shadow_reuse {
            for (index, groups) in draws.groups.iter().enumerate() {
                if *groups > 0 {
                    pass.set_bind_group(0, &draws.binds[index][2], &[]);
                    pass.dispatch_workgroups(*groups, 1, 1);
                }
            }
        }
    }
}

pub(crate) struct ShadowPass;

impl Pass for ShadowPass {
    fn name(&self) -> &'static str {
        "shadow"
    }

    fn reads(&self) -> &'static [&'static str] {
        &["cull"]
    }

    fn writes(&self) -> &'static [&'static str] {
        &["shadow"]
    }

    fn record(&mut self, cx: &mut PassContext<'_>) {
        let draws = cx.draws;
        let timestamp_writes = cx.timestamp_writes();
        let mut pass = cx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("shadow"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &draws.shadow_view,
                depth_ops: Some(wgpu::Operations {
                    load: if draws.shadow_reuse {
                        wgpu::LoadOp::Load
                    } else {
                        wgpu::LoadOp::Clear(1.0)
                    },
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if !draws.shadows_enabled || draws.batches.is_empty() || draws.shadow_reuse {
            return;
        }
        pass.set_pipeline(&draws.shadow_pipeline);
        pass.set_bind_group(0, &draws.shadow_bind_group, &[]);
        draw_indirect(&mut pass, draws, &draws.shadow_compact, 2);
    }
}

pub(crate) struct DepthPass;

impl Pass for DepthPass {
    fn name(&self) -> &'static str {
        "depth"
    }

    fn reads(&self) -> &'static [&'static str] {
        &["cull", "depth"]
    }

    fn writes(&self) -> &'static [&'static str] {
        &["depth"]
    }

    fn record(&mut self, cx: &mut PassContext<'_>) {
        let draws = cx.draws;
        let timestamp_writes = cx.timestamp_writes();
        let mut pass = cx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("depth"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: cx.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if draws.batches.is_empty() {
            return;
        }
        pass.set_pipeline(&draws.depth_pipeline);
        pass.set_bind_group(0, &draws.bind_group, &[]);
        draw_indirect(&mut pass, draws, &draws.camera_compact, 0);
    }
}

pub(crate) struct HzbPass;

impl Pass for HzbPass {
    fn name(&self) -> &'static str {
        "hzb"
    }

    fn reads(&self) -> &'static [&'static str] {
        &["depth"]
    }

    fn writes(&self) -> &'static [&'static str] {
        &["hzb"]
    }

    fn record(&mut self, cx: &mut PassContext<'_>) {
        let draws = cx.draws;
        if !draws.build_hzb {
            return;
        }
        let hzb = &draws.hzb;
        let mut pass = cx.encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("hzb"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&draws.hzb_l0_pipeline);
        pass.set_bind_group(0, &hzb.l0, &[]);
        let (width, height) = hzb.dims[0];
        pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
        pass.set_pipeline(&draws.hzb_down_pipeline);
        for (index, bind) in hzb.down.iter().enumerate() {
            let (width, height) = hzb.dims[index + 1];
            pass.set_bind_group(0, bind, &[]);
            pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
        }
    }
}

pub(crate) struct CullPhase2Pass;

impl Pass for CullPhase2Pass {
    fn name(&self) -> &'static str {
        "cull2"
    }

    fn reads(&self) -> &'static [&'static str] {
        &["hzb", "cull"]
    }

    fn writes(&self) -> &'static [&'static str] {
        &["cull2"]
    }

    fn record(&mut self, cx: &mut PassContext<'_>) {
        let draws = cx.draws;
        if !draws.occlusion_active || draws.batches.is_empty() {
            return;
        }
        let mut pass = cx.encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("cull2"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&draws.cull_pipeline);
        for (index, groups) in draws.groups.iter().enumerate() {
            if *groups > 0 {
                pass.set_bind_group(0, &draws.binds[index][1], &[]);
                pass.dispatch_workgroups(*groups, 1, 1);
            }
        }
    }
}

pub(crate) struct DepthPass2;

impl Pass for DepthPass2 {
    fn name(&self) -> &'static str {
        "depth2"
    }

    fn reads(&self) -> &'static [&'static str] {
        &["cull2", "depth"]
    }

    fn writes(&self) -> &'static [&'static str] {
        &["depth2"]
    }

    fn record(&mut self, cx: &mut PassContext<'_>) {
        let draws = cx.draws;
        if !draws.occlusion_active || draws.batches.is_empty() {
            return;
        }
        let timestamp_writes = cx.timestamp_writes();
        let mut pass = cx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("depth2"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: cx.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&draws.depth_pipeline);
        pass.set_bind_group(0, &draws.bind_group, &[]);
        draw_indirect(&mut pass, draws, &draws.camera_compact2, 1);
    }
}

fn draw_indirect(
    pass: &mut wgpu::RenderPass<'_>,
    draws: &crate::renderer::DrawResources,
    instances: &wgpu::Buffer,
    args_index: usize,
) {
    for (index, batch) in draws.batches.iter().enumerate() {
        if batch.count == 0 {
            continue;
        }
        let mesh = &draws.meshes[batch.mesh];
        let slot = &draws.slots[index];
        for (lod_index, lod) in mesh.lods.iter().enumerate() {
            let offset = (lod_index as u64 * draws.capacity as u64 + u64::from(batch.first))
                * crate::renderer::INSTANCE_BYTES;
            pass.set_vertex_buffer(0, lod.vertices.slice(..));
            pass.set_vertex_buffer(1, instances.slice(offset..));
            pass.set_index_buffer(lod.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed_indirect(
                &slot.args[args_index],
                lod_index as u64 * crate::renderer::ARGS_BYTES,
            );
        }
    }
}

pub(crate) struct AoPass;

impl Pass for AoPass {
    fn name(&self) -> &'static str {
        "ao"
    }

    fn reads(&self) -> &'static [&'static str] {
        &["depth"]
    }

    fn writes(&self) -> &'static [&'static str] {
        &["ao"]
    }

    fn record(&mut self, cx: &mut PassContext<'_>) {
        let draws = cx.draws;
        let timestamp_writes = cx.timestamp_writes();
        let mut pass = cx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ao"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &draws.ao_raw,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let Some(bind) = draws.ao_bind.as_ref() else {
            return;
        };
        if !draws.ambient_occlusion_enabled {
            return;
        }
        pass.set_pipeline(&draws.ao_pipeline);
        pass.set_bind_group(0, bind, &[]);
        pass.draw(0..3, 0..1);
    }
}

pub(crate) struct AoBlurPass;

impl Pass for AoBlurPass {
    fn name(&self) -> &'static str {
        "ao_blur"
    }

    fn reads(&self) -> &'static [&'static str] {
        &["ao"]
    }

    fn writes(&self) -> &'static [&'static str] {
        &["ao_blur"]
    }

    fn record(&mut self, cx: &mut PassContext<'_>) {
        let draws = cx.draws;
        let timestamp_writes = cx.timestamp_writes();
        let mut pass = cx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ao blur"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &draws.ao_blur,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let Some(bind) = draws.ao_blur_bind.as_ref() else {
            return;
        };
        if !draws.ambient_occlusion_enabled {
            return;
        }
        pass.set_pipeline(&draws.ao_blur_pipeline);
        pass.set_bind_group(0, bind, &[]);
        pass.draw(0..3, 0..1);
    }
}

pub(crate) fn default_passes() -> Vec<Box<dyn Pass>> {
    vec![
        Box::new(CullPass),
        Box::new(ShadowPass),
        Box::new(ClearPass),
        Box::new(DepthPass),
        Box::new(HzbPass),
        Box::new(CullPhase2Pass),
        Box::new(DepthPass2),
        Box::new(AoPass),
        Box::new(AoBlurPass),
        Box::new(OpaquePass),
        Box::new(TransparentPass),
        Box::new(PostPass),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Named {
        name: &'static str,
        reads: &'static [&'static str],
        writes: &'static [&'static str],
    }

    impl Pass for Named {
        fn name(&self) -> &'static str {
            self.name
        }

        fn reads(&self) -> &'static [&'static str] {
            self.reads
        }

        fn writes(&self) -> &'static [&'static str] {
            self.writes
        }

        fn record(&mut self, _cx: &mut PassContext<'_>) {}
    }

    fn boxed(
        name: &'static str,
        reads: &'static [&'static str],
        writes: &'static [&'static str],
    ) -> Box<dyn Pass> {
        Box::new(Named {
            name,
            reads,
            writes,
        })
    }

    #[test]
    fn a_read_must_follow_a_write() {
        let error = RenderGraph::new(vec![boxed("a", &["color"], &[])])
            .err()
            .expect("the read has no writer");
        assert_eq!(
            error,
            GraphError::UnwrittenResource {
                pass: "a",
                resource: "color",
            }
        );
    }

    #[test]
    fn a_write_must_come_before_the_read_in_the_order() {
        let reversed = vec![
            boxed("reader", &["color"], &[]),
            boxed("writer", &[], &["color"]),
        ];
        assert!(RenderGraph::new(reversed).is_err());
        let ordered = vec![
            boxed("writer", &[], &["color"]),
            boxed("reader", &["color"], &[]),
        ];
        assert!(RenderGraph::new(ordered).is_ok());
    }

    #[test]
    fn pass_names_must_be_unique() {
        let duplicated = vec![boxed("a", &[], &[]), boxed("a", &[], &[])];
        assert_eq!(
            RenderGraph::new(duplicated).err(),
            Some(GraphError::DuplicatePass("a"))
        );
    }

    #[test]
    fn the_default_pass_order_is_valid_and_reports_each_pass() {
        let graph = RenderGraph::new(default_passes()).expect("default passes are valid");
        assert_eq!(
            graph.pass_names(),
            vec![
                "cull",
                "shadow",
                "clear",
                "depth",
                "hzb",
                "cull2",
                "depth2",
                "ao",
                "ao_blur",
                "opaque",
                "transparent",
                "post"
            ]
        );
        let report = graph.report();
        for name in [
            "cull",
            "shadow",
            "clear",
            "depth",
            "hzb",
            "cull2",
            "ao",
            "ao_blur",
            "depth2",
            "opaque",
            "transparent",
            "post",
        ] {
            assert!(report.contains(name), "report lists {name}");
        }
        assert_eq!(graph.timing("opaque").unwrap().calls, 0);
    }

    #[test]
    fn timing_averages_over_calls() {
        let mut timing = Timing::default();
        assert_eq!(timing.average(), Duration::ZERO);
        timing.add(Duration::from_micros(10));
        timing.add(Duration::from_micros(30));
        assert_eq!(timing.calls, 2);
        assert_eq!(timing.last, Duration::from_micros(30));
        assert_eq!(timing.average(), Duration::from_micros(20));
    }
}
