use std::fmt::Write as _;
use std::time::{Duration, Instant};

use crate::renderer::DrawResources;

pub struct PassContext<'a> {
    pub encoder: &'a mut wgpu::CommandEncoder,
    pub color: &'a wgpu::TextureView,
    pub depth: &'a wgpu::TextureView,
    pub(crate) draws: &'a DrawResources,
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
}

pub struct RenderGraph {
    slots: Vec<Slot>,
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
            slots: passes
                .into_iter()
                .map(|pass| Slot {
                    pass,
                    timing: Timing::default(),
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

    pub(crate) fn execute(&mut self, cx: &mut PassContext<'_>) {
        for slot in &mut self.slots {
            let start = Instant::now();
            slot.pass.record(cx);
            slot.timing.add(start.elapsed());
        }
    }

    pub fn report(&self) -> String {
        let mut out = String::from("pass             last (us)   average (us)   calls\n");
        for slot in &self.slots {
            let _ = writeln!(
                out,
                "{:<16} {:>9.1} {:>13.1} {:>7}",
                slot.pass.name(),
                slot.timing.last.as_secs_f64() * 1e6,
                slot.timing.average().as_secs_f64() * 1e6,
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
            timestamp_writes: None,
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
        &["color", "depth"]
    }

    fn writes(&self) -> &'static [&'static str] {
        &["color", "depth"]
    }

    fn record(&mut self, cx: &mut PassContext<'_>) {
        if cx.draws.instance_count == 0 {
            return;
        }
        let draws = cx.draws;
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
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&draws.pipeline);
        pass.set_bind_group(0, &draws.bind_group, &[]);
        pass.set_vertex_buffer(0, draws.vertices.slice(..));
        pass.set_vertex_buffer(1, draws.instances.slice(..));
        pass.set_index_buffer(draws.indices.slice(..), wgpu::IndexFormat::Uint16);
        if draws.unbatched {
            for instance in 0..draws.instance_count {
                pass.draw_indexed(0..draws.index_count, 0, instance..instance + 1);
            }
        } else {
            pass.draw_indexed(0..draws.index_count, 0, 0..draws.instance_count);
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

    fn record(&mut self, _cx: &mut PassContext<'_>) {}
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

    fn record(&mut self, _cx: &mut PassContext<'_>) {}
}

pub(crate) fn default_passes() -> Vec<Box<dyn Pass>> {
    vec![
        Box::new(ClearPass),
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
            vec!["clear", "opaque", "transparent", "post"]
        );
        let report = graph.report();
        for name in ["clear", "opaque", "transparent", "post"] {
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
