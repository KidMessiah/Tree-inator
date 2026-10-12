//! Shading and compositing on the GPU with wgpu compute shaders. This follows `engine::shade_region` and
//! `compose::compose` step by step; the CPU code stays the reference and the fallback. Planning and
//! stamping still run on the CPU, and their results are uploaded here.

use crate::compose::ComposeOptions;
use crate::engine::{Plan, Region, Stamped};
use crate::field::half_radii;
use crate::params::*;
use rayon::prelude::*;
use std::collections::HashMap;
use wgpu::util::DeviceExt;

/// Integer settings, then float settings, in the order they sit in the settings block.
const U_PARAMS: &[&str] = &[
    "w", "h", "ox", "oy", "sd", "realistic", "flat", "clip_on", "snow_mode", "fire_mode", "main_species", "n_species", "sdf_w", "sdf_h",
    "any_shape", "ins_gw", "ins_gh", "glow_w", "glow_h", "glow_on", "pattern", "uh_w", "uh_h", "ug_w", "ug_h", "plan_fine",
];
const F_PARAMS: &[&str] = &[
    "s", "inv", "map_w", "map_h", "lvx", "lvy", "lvz", "soft", "fall", "rel_scale", "variety", "mottling", "mottle_size", "dappled",
    "tree_lean", "crown_tone", "crown_tone_size", "crown_shade", "cluster_tone", "depth_strength", "gamma", "gain", "crown_var",
    "canopy_ao", "translucency", "lumps", "snow_amount", "snow_ground", "snow_tops", "snow_reach", "snow_c", "snow_s", "snow_patch", "snow_r", "snow_g", "snow_b",
    "fire_amount", "fire_flames", "fire_char", "fire_embers", "fire_glow", "fire_reach", "fire_c", "fire_s", "fire_patch", "grass_r",
    "grass_g", "grass_b", "acc_r", "acc_g", "acc_b", "feather", "plan_s", "ins_cs", "band", "edge_warp", "edge_noise", "pattern_k",
    "pattern_sc", "pattern_scale_raw", "branch_width", "haze_reach", "haze_opacity", "grass_reach", "grass_density",
];
const N_F: usize = 10;
/// Integer slots; the last two hold atomics.
const N_U: usize = 7;
const WG: u32 = 256;
const ENTRIES: &[&str] = &[
    "k_top", "k_horizon", "k_shadow_raw", "k_box_h", "k_box_v", "k_down2", "k_up2", "k_mask", "k_clip", "k_glow_src", "k_shade_tree",
    "k_grass", "k_compose", "k_stamp_z", "k_stamp_win", "k_stamp_attr", "k_depth_coarse", "k_upsample", "k_haze", "k_grass_imp",
    "k_grass_a", "k_area", "k_smooth",
];

fn n_param_vec4() -> usize {
    (U_PARAMS.len() + F_PARAMS.len()).div_ceil(4)
}

/// The WGSL settings block and one accessor per setting, put in front of the kernels.
fn prelude() -> String {
    let mut s = format!("struct Params {{ v: array<vec4<f32>, {}> }};\n", n_param_vec4());
    for (i, name) in U_PARAMS.iter().chain(F_PARAMS).enumerate() {
        let at = format!("P.v[{}][{}]", i / 4, i % 4);
        if i < U_PARAMS.len() {
            s += &format!("fn p_{name}() -> u32 {{ return bitcast<u32>({at}); }}\n");
        } else {
            s += &format!("fn p_{name}() -> f32 {{ return {at}; }}\n");
        }
    }
    s
}

/// The settings block, filled by name.
struct Params(Vec<f32>);

impl Params {
    fn new() -> Params {
        Params(vec![0.0; n_param_vec4() * 4])
    }
    fn u(&mut self, name: &str, v: u32) {
        let i = U_PARAMS.iter().position(|n| *n == name).expect("integer setting");
        self.0[i] = f32::from_bits(v);
    }
    fn f(&mut self, name: &str, v: f32) {
        let i = U_PARAMS.len() + F_PARAMS.iter().position(|n| *n == name).expect("float setting");
        self.0[i] = v;
    }
}

/// Per dispatch values, matching `Step` in the kernels.
#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct Step {
    f: [f32; 4],
    g: [f32; 4],
    u: [u32; 4],
    t: [u32; 4],
}

fn step_u(u: [u32; 4]) -> Step {
    Step { u, ..Default::default() }
}

/// A GPU and the compiled kernels.
pub struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    layout0: wgpu::BindGroupLayout,
    layout1: wgpu::BindGroupLayout,
    pipes: HashMap<&'static str, wgpu::ComputePipeline>,
    dummy_f: Vec<wgpu::Buffer>,
    dummy_u: Vec<wgpu::Buffer>,
    dummy_tex: wgpu::TextureView,
}

/// Shaded layers kept on the GPU, ready to composite as often as grading changes.
pub struct GpuLayers {
    pub w: usize,
    pub h: usize,
    pub scale: f32,
    pub tree_counts: [usize; 4],
    pub fit_inset: f32,
    pub trees: Vec<TreeRec>,
    params: wgpu::Buffer,
    haze: wgpu::Buffer,
    haze_color: [f32; 3],
    ground: Option<wgpu::Buffer>,
    /// Grass, shrubs, canopy, lone trees, pebbles.
    colors: Vec<wgpu::Buffer>,
    shadows: Vec<Option<wgpu::Buffer>>,
}

/// Uploads shared by every band of one render.
struct Shared {
    ramp: wgpu::Buffer,
    sdf: wgpu::Buffer,
    ins: wgpu::Buffer,
    uhaze: wgpu::Buffer,
    ugrass: wgpu::Buffer,
    atlas: Option<Atlas>,
}

/// What one band needs before it can be shaded.
struct BandInputs {
    heights: Vec<wgpu::Buffer>,
    w1: Vec<wgpu::Buffer>,
    w2: Vec<wgpu::Buffer>,
    painted_any: Vec<bool>,
    depth: wgpu::Buffer,
    haze_a: wgpu::Buffer,
    grass_a: wgpu::Buffer,
    area: wgpu::Buffer,
}

/// Every sprite a render uses, packed for the GPU, and each layer's trees as 8 words: x0, y0, atlas offset,
/// size, turns | mirror << 2, height scale, class byte template, unused.
struct Atlas {
    z: wgpu::Buffer,
    w: wgpu::Buffer,
    c: wgpu::Buffer,
    layers: Vec<Vec<[u32; 16]>>,
}

impl Atlas {
    fn build(gpu: &Gpu, layers: Vec<Vec<crate::engine::SpriteInstance>>) -> Atlas {
        let mut at: HashMap<*const crate::sprite::Sprite, u32> = HashMap::new();
        let (mut z, mut w, mut c) = (Vec::new(), Vec::new(), Vec::new());
        let mut out = Vec::new();
        for layer in &layers {
            let mut words = Vec::with_capacity(layer.len());
            for q in layer {
                let sp = &q.spr;
                let off = *at.entry(std::sync::Arc::as_ptr(sp)).or_insert_with(|| {
                    let off = z.len() as u32;
                    for i in 0..sp.size * sp.size {
                        z.push(sp.z[i]);
                        let nx = (sp.nx[i] * 80.0).round().clamp(-127.0, 127.0) as i8 as u8 as u32;
                        let ny = (sp.ny[i] * 80.0).round().clamp(-127.0, 127.0) as i8 as u8 as u32;
                        let tone = (sp.tone[i].clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
                        let depth = (sp.depth[i].clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
                        w.push(nx | ny << 8 | tone << 16 | depth << 24);
                        c.push(sp.cls[i] as u32);
                    }
                    off
                });
                let template = (q.painted as u32) << 7 | (q.species as u32) << 8 | (q.tone as u16 as u32) << 16;
                let mut word = [0u32; 16];
                word[..7].copy_from_slice(&[q.x0 as u32, q.y0 as u32, off, sp.size as u32, q.k | (q.flip as u32) << 2, q.zs.to_bits(), template]);
                word[14] = q.top.to_bits();
                word[15] = q.weave.to_bits();
                if let Some(f) = &q.fac {
                    word[7] = 1;
                    word[8..14].copy_from_slice(&crate::engine::shy_pack(f));
                }
                words.push(word);
            }
            out.push(words);
        }
        if std::env::var_os("FOREST_TIMING").is_some() {
            eprintln!("  atlas: {} sprites, {:.1} MB", at.len(), z.len() as f32 * 12.0 / 1e6);
        }
        Atlas { z: gpu.upload(&z, "atlas heights"), w: gpu.upload(&w, "atlas bytes"), c: gpu.upload(&c, "atlas classes"), layers: out }
    }
}

/// Pixels per band when shading on the GPU.
const BAND_PX: usize = 4_000_000;

/// A composited image on the GPU. `view` reads the gamma encoded values as they are, the way egui expects.
pub struct GpuImage {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub w: usize,
    pub h: usize,
}

fn storage_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only }, has_dynamic_offset: false, min_binding_size: None },
        count: None,
    }
}

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
        count: None,
    }
}

impl Gpu {
    /// A GPU of its own, for the command line and tests. None when there is no usable adapter.
    pub fn headless() -> Option<Gpu> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .ok()?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("forest gpu"),
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .ok()?;
        Some(Gpu::new(device, queue))
    }

    /// Use a device made elsewhere, such as the app window's. It needs the adapter's full limits.
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Gpu {
        let layout0 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shared"),
            entries: &[
                uniform_entry(0),
                uniform_entry(1),
                storage_entry(2, true),
                storage_entry(3, true),
                storage_entry(4, true),
                storage_entry(5, true),
                storage_entry(6, true),
            ],
        });
        let mut e1 = vec![uniform_entry(0)];
        for b in 1..=(N_F + N_U) as u32 {
            e1.push(storage_entry(b, false));
        }
        e1.push(wgpu::BindGroupLayoutEntry {
            binding: (N_F + N_U + 1) as u32,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::StorageTexture {
                access: wgpu::StorageTextureAccess::WriteOnly,
                format: wgpu::TextureFormat::Rgba8Unorm,
                view_dimension: wgpu::TextureViewDimension::D2,
            },
            count: None,
        });
        let layout1 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("per dispatch"), entries: &e1 });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("kernels"),
            bind_group_layouts: &[Some(&layout0), Some(&layout1)],
            immediate_size: 0,
        });
        let src = prelude() + include_str!("kernels.wgsl") + include_str!("stamp.wgsl");
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("kernels"), source: wgpu::ShaderSource::Wgsl(src.into()) });
        let mut pipes = HashMap::new();
        for e in ENTRIES {
            let p = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(e),
                layout: Some(&pl),
                module: &module,
                entry_point: Some(e),
                compilation_options: Default::default(),
                cache: None,
            });
            pipes.insert(*e, p);
        }
        let small = |label| {
            device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: 16, usage: wgpu::BufferUsages::STORAGE, mapped_at_creation: false })
        };
        let dummy_f = (0..N_F).map(|_| small("dummy f")).collect();
        let dummy_u = (0..N_U).map(|_| small("dummy u")).collect();
        let dummy_tex = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("dummy image"),
                size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::STORAGE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());
        Gpu { device, queue, layout0, layout1, pipes, dummy_f, dummy_u, dummy_tex }
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    fn buffer(&self, words: usize, label: &str) -> wgpu::Buffer {
        self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: (words.max(4) * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn upload<T: bytemuck::Pod>(&self, data: &[T], label: &str) -> wgpu::Buffer {
        if data.is_empty() {
            return self.buffer(4, label);
        }
        self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents: bytemuck::cast_slice(data),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
        })
    }

    fn uniform<T: bytemuck::Pod>(&self, data: &[T]) -> wgpu::Buffer {
        self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("uniform"),
            contents: bytemuck::cast_slice(data),
            usage: wgpu::BufferUsages::UNIFORM,
        })
    }

    fn group0(&self, params: &wgpu::Buffer, grades: &wgpu::Buffer, shared: &Shared) -> wgpu::BindGroup {
        self.group0_with(params, grades, [&shared.ramp, &shared.sdf, &shared.ins, &shared.uhaze, &shared.ugrass])
    }

    fn group0_with(&self, params: &wgpu::Buffer, grades: &wgpu::Buffer, rest: [&wgpu::Buffer; 5]) -> wgpu::BindGroup {
        let bufs = [params, grades, rest[0], rest[1], rest[2], rest[3], rest[4]];
        let entries: Vec<wgpu::BindGroupEntry> =
            bufs.iter().enumerate().map(|(i, b)| wgpu::BindGroupEntry { binding: i as u32, resource: b.as_entire_binding() }).collect();
        self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("shared"), layout: &self.layout0, entries: &entries })
    }

    /// One kernel over `threads` items (or a single workgroup when `threads` is 0). `f` and `u` give
    /// (slot, buffer); unused slots get placeholders.
    #[allow(clippy::too_many_arguments)]
    fn run(
        &self,
        enc: &mut wgpu::CommandEncoder,
        g0: &wgpu::BindGroup,
        entry: &str,
        step: Step,
        threads: usize,
        f: &[(usize, &wgpu::Buffer)],
        u: &[(usize, &wgpu::Buffer)],
        tex: Option<&wgpu::TextureView>,
    ) {
        let groups = if threads == 0 {
            (1, 1)
        } else {
            let n = (threads as u32).div_ceil(WG);
            let x = n.min(65535);
            (x, n.div_ceil(x))
        };
        self.run_groups_tex(enc, g0, entry, step, groups, f, u, tex);
    }

    /// One kernel over a grid of workgroups.
    #[allow(clippy::too_many_arguments)]
    fn run_groups(&self, enc: &mut wgpu::CommandEncoder, g0: &wgpu::BindGroup, entry: &str, step: Step, groups: (u32, u32), f: &[(usize, &wgpu::Buffer)], u: &[(usize, &wgpu::Buffer)]) {
        self.run_groups_tex(enc, g0, entry, step, groups, f, u, None);
    }

    #[allow(clippy::too_many_arguments)]
    fn run_groups_tex(
        &self,
        enc: &mut wgpu::CommandEncoder,
        g0: &wgpu::BindGroup,
        entry: &str,
        step: Step,
        groups: (u32, u32),
        f: &[(usize, &wgpu::Buffer)],
        u: &[(usize, &wgpu::Buffer)],
        tex: Option<&wgpu::TextureView>,
    ) {
        let sb = self.uniform(&[step]);
        let mut fs: Vec<&wgpu::Buffer> = self.dummy_f.iter().collect();
        for (k, b) in f {
            fs[*k] = b;
        }
        let mut us: Vec<&wgpu::Buffer> = self.dummy_u.iter().collect();
        for (k, b) in u {
            us[*k] = b;
        }
        let mut entries = vec![wgpu::BindGroupEntry { binding: 0, resource: sb.as_entire_binding() }];
        for (k, b) in fs.iter().chain(us.iter()).enumerate() {
            entries.push(wgpu::BindGroupEntry { binding: k as u32 + 1, resource: b.as_entire_binding() });
        }
        entries.push(wgpu::BindGroupEntry {
            binding: (N_F + N_U + 1) as u32,
            resource: wgpu::BindingResource::TextureView(tex.unwrap_or(&self.dummy_tex)),
        });
        let g1 = self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some(entry), layout: &self.layout1, entries: &entries });
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some(entry), timestamp_writes: None });
        pass.set_pipeline(&self.pipes[entry]);
        pass.set_bind_group(0, g0, &[]);
        pass.set_bind_group(1, &g1, &[]);
        pass.dispatch_workgroups(groups.0, groups.1, 1);
    }

    /// `field::Field::blur` on the GPU: `src` (w x h) into `dst`. `tmp` is full size; `h1`, `h2` half size.
    #[allow(clippy::too_many_arguments)]
    fn blur(&self, enc: &mut wgpu::CommandEncoder, g0: &wgpu::BindGroup, src: &wgpu::Buffer, dst: &wgpu::Buffer, w: usize, h: usize, radius: f32, tmp: &wgpu::Buffer, h1: &wgpu::Buffer, h2: &wgpu::Buffer) {
        if radius < 0.5 {
            enc.copy_buffer_to_buffer(src, 0, dst, 0, (w * h * 4) as u64);
            return;
        }
        let r = ((radius * 0.9).round() as usize).max(1);
        let (ww, hh) = (w as u32, h as u32);
        if radius >= 8.0 && w >= 64 && h >= 64 {
            let (hw, hhh) = (w.div_ceil(2) as u32, h.div_ceil(2) as u32);
            let n2 = (hw * hhh) as usize;
            self.run(enc, g0, "k_down2", step_u([ww, hh, hw, hhh]), n2, &[(0, src), (1, h1)], &[], None);
            let rs = half_radii(r);
            self.box3(enc, g0, h1, h2, hw, hhh, rs);
            self.run(enc, g0, "k_up2", step_u([ww, hh, hw, hhh]), w * h, &[(0, h1), (1, dst)], &[], None);
        } else {
            enc.copy_buffer_to_buffer(src, 0, dst, 0, (w * h * 4) as u64);
            self.box3(enc, g0, dst, tmp, ww, hh, [r; 3]);
        }
    }

    /// Three passes along rows then three down columns, in place in `a` with `b` as scratch.
    #[allow(clippy::too_many_arguments)]
    fn box3(&self, enc: &mut wgpu::CommandEncoder, g0: &wgpu::BindGroup, a: &wgpu::Buffer, b: &wgpu::Buffer, w: u32, h: u32, rs: [usize; 3]) {
        let n = (w * h) as usize;
        for (k, entry) in ["k_box_h", "k_box_v"].iter().enumerate() {
            let _ = k;
            self.run(enc, g0, entry, step_u([w, h, rs[0] as u32, 0]), n, &[(0, a), (1, b)], &[], None);
            self.run(enc, g0, entry, step_u([w, h, rs[1] as u32, 0]), n, &[(0, b), (1, a)], &[], None);
            self.run(enc, g0, entry, step_u([w, h, rs[2] as u32, 0]), n, &[(0, a), (1, b)], &[], None);
            enc.copy_buffer_to_buffer(b, 0, a, 0, (n * 4) as u64);
        }
    }

    /// Shade a region stamped on the CPU, like `engine::shade_region`, and keep the layers on the GPU.
    pub fn shade(&self, project: &Project, plan: &Plan, stamped: &Stamped) -> GpuLayers {
        let shared = self.shared(project, plan, None);
        self.bands(project, plan, stamped.scale, stamped.region, &shared, &mut |_, _, y0, y1| self.cpu_inputs(project, plan, stamped, y0, y1))
    }

    /// Stamp and shade a region entirely on the GPU, from the plan alone.
    pub fn render(&self, project: &Project, plan: &Plan, scale: f32, region: Region) -> GpuLayers {
        let s = scale * project.gen.scale.clamp(0.1, 4.0);
        let clock = std::time::Instant::now();
        let rect = [region.x0 as f32, region.y0 as f32, (region.x0 + region.w) as f32, (region.y0 + region.h) as f32];
        let fac = plan.shy_factors(&crate::engine::Shy::of(&project.gen), 1.0 / project.gen.scale.clamp(0.1, 4.0));
        let atlas = Atlas::build(self, (0..4).map(|j| plan.instances(j, &project.gen.depth, s, Some(rect), if j == 1 { fac.as_deref().map(|v| v.as_slice()) } else { None })).collect());
        if std::env::var_os("FOREST_TIMING").is_some() {
            eprintln!("  {:>7.2}s  atlas", clock.elapsed().as_secs_f32());
        }
        let shared = self.shared(project, plan, Some(atlas));
        self.bands(project, plan, scale, region, &shared, &mut |enc, g0, y0, y1| self.gpu_inputs(enc, g0, project, plan, scale, region, y0, y1, &shared))
    }

    fn shared(&self, project: &Project, plan: &Plan, atlas: Option<Atlas>) -> Shared {
        let mut ramp: Vec<[f32; 4]> = Vec::new();
        for sp in &project.species {
            for c in sp.ramp {
                ramp.push([c[0], c[1], c[2], 0.0]);
            }
            // the bark entry's last slot flags a species whose sprites carry their own colouring
            ramp.push([sp.bark[0], sp.bark[1], sp.bark[2], if sp.plain_colours() { 1.0 } else { 0.0 }]);
        }
        Shared {
            ramp: self.upload(&ramp, "ramps"),
            sdf: if plan.any_shape { self.upload(&plan.sdf.data, "sdf") } else { self.buffer(4, "sdf") },
            ins: self.upload(&plan.ins, "pull-in"),
            uhaze: self.upload(&plan.u_haze.data, "haze noise"),
            ugrass: self.upload(&plan.u_grass.data, "grass noise"),
            atlas,
        }
    }

    /// Run a region through in bands of rows, each with enough overlap for shadows and blurs, so GPU
    /// memory stays at the finished layers plus one band. `inputs` makes a band's stamps and ground cover.
    fn bands(
        &self,
        project: &Project,
        plan: &Plan,
        scale: f32,
        region: Region,
        shared: &Shared,
        inputs: &mut dyn FnMut(&mut wgpu::CommandEncoder, &wgpu::BindGroup, usize, usize) -> BandInputs,
    ) -> GpuLayers {
        let (w, h) = (region.w, region.h);
        let n = w * h;
        let (up, down) = crate::render::strip_halo(project, plan, scale);
        // FOREST_BAND_PX overrides the band size, for tuning on other cards
        let band_px = std::env::var("FOREST_BAND_PX").ok().and_then(|v| v.parse().ok()).unwrap_or(BAND_PX);
        let rows = ((band_px / w.max(1)).max(64)).div_ceil(4) * 4;
        let mut full: Option<GpuLayers> = None;
        let timing = std::env::var_os("FOREST_TIMING").is_some();
        let clock = std::time::Instant::now();
        let mut cy = 0;
        while cy < h {
            let core = rows.min(h - cy);
            // bands start on a multiple of 4 rows, so coarse grids line up between bands
            let y0 = cy.saturating_sub(up) / 4 * 4;
            let y1 = (cy + core + down).min(h);
            let params = self.uniform(&self.band_params(project, plan, scale, region, y0, y1).0);
            let grades = self.uniform(&[[0f32; 4]; 40]);
            let g0 = self.group0(&params, &grades, shared);
            let mut enc = self.device.create_command_encoder(&Default::default());
            let inp = inputs(&mut enc, &g0, y0, y1);
            let band = self.shade_band(&mut enc, &g0, project, plan, scale, region, y0, y1, params, inp);
            let out = full.get_or_insert_with(|| GpuLayers {
                params: self.uniform(&{
                    let mut p = Params::new();
                    p.u("w", w as u32);
                    p.u("h", h as u32);
                    p.0
                }),
                haze: self.buffer(n, "haze"),
                ground: band.ground.as_ref().map(|_| self.buffer(n, "ground shadow")),
                colors: band.colors.iter().map(|_| self.buffer(2 * n, "layer colour")).collect(),
                shadows: band.shadows.iter().map(|sh| sh.as_ref().map(|_| self.buffer(n, "shadow"))).collect(),
                h,
                ..band
            });
            let copy = |enc: &mut wgpu::CommandEncoder, src: &wgpu::Buffer, dst: &wgpu::Buffer, words: usize| {
                let at = ((cy - y0) * w * words * 4) as u64;
                enc.copy_buffer_to_buffer(src, at, dst, (cy * w * words * 4) as u64, (core * w * words * 4) as u64);
            };
            copy(&mut enc, &band.haze, &out.haze, 1);
            if let (Some(a), Some(b)) = (&band.ground, &out.ground) {
                copy(&mut enc, a, b, 1);
            }
            for (a, b) in band.colors.iter().zip(&out.colors) {
                copy(&mut enc, a, b, 2);
            }
            for (a, b) in band.shadows.iter().zip(&out.shadows) {
                if let (Some(a), Some(b)) = (a, b) {
                    copy(&mut enc, a, b, 1);
                }
            }
            self.queue.submit([enc.finish()]);
            // finish this band before the next, so its scratch memory is given back
            self.wait();
            if timing {
                eprintln!("  {:>7.2}s  gpu band done", clock.elapsed().as_secs_f32());
            }
            cy += core;
        }
        full.expect("at least one band")
    }

    /// The settings block for rows `y0..y1` of a region.
    fn band_params(&self, project: &Project, plan: &Plan, scale: f32, region: Region, y0: usize, y1: usize) -> Params {
        let g = &project.gen;
        let light = &project.light;
        let f = g.scale.clamp(0.1, 4.0);
        let s = scale * f;
        let (w, h) = (region.w, y1 - y0);
        let realistic = g.edge_mode == EdgeMode::Realistic;
        let clip_on = g.contain && plan.any_shape && g.contain_clip;
        // settings
        let mut p = Params::new();
        p.u("w", w as u32);
        p.u("h", h as u32);
        p.u("ox", region.x0 as u32);
        p.u("oy", (region.y0 + y0) as u32);
        p.u("pattern", match g.pattern {
            Pattern::Plain => 0,
            Pattern::Fragmented => 1,
            Pattern::Branching => 2,
            Pattern::Clumps => 3,
            Pattern::Lobed => 4,
        });
        p.u("uh_w", plan.u_haze.w as u32);
        p.u("uh_h", plan.u_haze.h as u32);
        p.u("ug_w", plan.u_grass.w as u32);
        p.u("ug_h", plan.u_grass.h as u32);
        p.u("plan_fine", plan.fine as u32);
        for (k, v) in [
            ("band", g.shape_softness.max(5.0)),
            ("edge_warp", g.edge_warp),
            ("edge_noise", g.edge_noise),
            ("pattern_k", g.pattern_strength.clamp(0.0, 1.0)),
            ("pattern_sc", g.pattern_scale.max(20.0)),
            ("pattern_scale_raw", g.pattern_scale),
            ("branch_width", g.branch_width),
            ("haze_reach", g.haze_reach),
            ("haze_opacity", g.haze_opacity),
            ("grass_reach", g.grass_reach),
            ("grass_density", g.grass_density),
        ] {
            p.f(k, v);
        }
        p.u("sd", (g.seed as u32).wrapping_mul(7919));
        p.u("realistic", realistic as u32);
        p.u("flat", light.flat as u32);
        p.u("clip_on", clip_on as u32);
        p.u("snow_mode", match g.snow.mode {
            SnowMode::Everywhere => 0,
            SnowMode::Edge => 1,
            SnowMode::Gradient => 2,
        });
        p.u("fire_mode", match g.fire.mode {
            FireMode::Everywhere => 0,
            FireMode::Edge => 1,
            FireMode::Front => 2,
        });
        p.u("main_species", g.main_species as u32);
        p.u("n_species", project.species.len().max(1) as u32);
        p.u("sdf_w", plan.sdf.w as u32);
        p.u("sdf_h", plan.sdf.h as u32);
        p.u("any_shape", plan.any_shape as u32);
        p.u("ins_gw", plan.gw as u32);
        p.u("ins_gh", plan.gh as u32);
        p.f("s", s);
        p.f("inv", 1.0 / s);
        p.f("map_w", project.map_w as f32 / f);
        p.f("map_h", project.map_h as f32 / f);
        let ang = light.shadow_angle.to_radians();
        let sun = ang + std::f32::consts::PI;
        let el = light.elevation.clamp(5.0, 90.0).to_radians();
        p.f("lvx", sun.cos() * el.cos());
        p.f("lvy", sun.sin() * el.cos());
        p.f("lvz", el.sin());
        let slope = 0.6 / light.shadow_length.max(0.05);
        let max_top = plan.max_height() * s;
        let steps = ((max_top / slope).ceil() as usize).clamp(1, 600);
        p.f("soft", 3.0 * s);
        p.f("fall", 1.0 / (steps as f32 + 6.0));
        p.f("rel_scale", 1.0 / (g.depth.shade_radius.max(2.0) * s * 0.8));
        let (ls, ds) = (&g.leaf, &g.depth);
        for (k, v) in [("variety", ls.variety), ("mottling", ls.mottling), ("mottle_size", ls.mottle_size), ("dappled", ls.dappled), ("tree_lean", ls.tree_lean)] {
            p.f(k, v);
        }
        for (k, v) in [
            ("crown_tone", ds.crown_tone),
            ("crown_tone_size", ds.crown_tone_size),
            ("crown_shade", ds.crown_shade),
            ("cluster_tone", ds.cluster_tone),
            ("depth_strength", ds.depth_strength),
            ("gamma", ds.gamma),
            ("gain", ds.gain),
            ("crown_var", ds.crown_var),
            ("canopy_ao", ds.canopy_ao),
            ("translucency", ds.translucency),
            ("lumps", ds.lumps),
        ] {
            p.f(k, v);
        }
        let sn = &g.snow;
        for (k, v) in [
            ("snow_amount", sn.amount),
            ("snow_ground", sn.ground),
            ("snow_tops", sn.tops),
            ("snow_reach", sn.reach),
            ("snow_c", sn.angle.to_radians().cos()),
            ("snow_s", sn.angle.to_radians().sin()),
            ("snow_patch", sn.patchiness),
            ("snow_r", sn.tint[0]),
            ("snow_g", sn.tint[1]),
            ("snow_b", sn.tint[2]),
        ] {
            p.f(k, v);
        }
        let fi = &g.fire;
        for (k, v) in [
            ("fire_amount", fi.amount),
            ("fire_flames", fi.flames),
            ("fire_char", fi.char),
            ("fire_embers", fi.embers),
            ("fire_glow", fi.glow),
            ("fire_reach", fi.reach),
            ("fire_c", fi.angle.to_radians().cos()),
            ("fire_s", fi.angle.to_radians().sin()),
            ("fire_patch", fi.patchiness),
        ] {
            p.f(k, v);
        }
        for (k, v) in [
            ("grass_r", g.grass_color[0]),
            ("grass_g", g.grass_color[1]),
            ("grass_b", g.grass_color[2]),
            ("acc_r", g.grass_accent[0]),
            ("acc_g", g.grass_accent[1]),
            ("acc_b", g.grass_accent[2]),
        ] {
            p.f(k, v);
        }
        p.f("feather", g.feather.max(0.02));
        p.f("plan_s", plan.s);
        p.f("ins_cs", plan.cs as f32);
        let glow_on = fi.amount > 0.0 && fi.glow > 0.0 && fi.flames > 0.0;
        let (gw, gh) = (w / 4 + 1, h / 4 + 1);
        p.u("glow_on", glow_on as u32);
        p.u("glow_w", gw as u32);
        p.u("glow_h", gh as u32);
        p
    }

    /// A band's inputs from CPU stamps.
    fn cpu_inputs(&self, project: &Project, plan: &Plan, stamped: &Stamped, y0: usize, y1: usize) -> BandInputs {
        let g = &project.gen;
        let clip_on = g.contain && plan.any_shape && g.contain_clip;
        let w = stamped.region.w;
        let (a, b) = (y0 * w, y1 * w);
        let heights: Vec<wgpu::Buffer> = stamped.stamps.iter().map(|st| self.upload(&st.height[a..b], "height")).collect();
        let packed: Vec<(Vec<u32>, Vec<u32>, bool)> = stamped
            .stamps
            .par_iter()
            .map(|st| {
                let w1: Vec<u32> = (a..b)
                    .into_par_iter()
                    .map(|i| (st.spr_nx[i] as u8 as u32) | (st.spr_ny[i] as u8 as u32) << 8 | (st.spr_tone[i] as u32) << 16 | (st.spr_depth[i] as u32) << 24)
                    .collect();
                let w2: Vec<u32> = (a..b)
                    .into_par_iter()
                    .map(|i| (st.spr_cls[i] as u32 & 0x7f) | (st.painted[i] as u32 & 1) << 7 | (st.species[i] as u32) << 8 | (st.tone[i] as u16 as u32) << 16)
                    .collect();
                (w1, w2, st.painted[a..b].iter().any(|v| *v > 0))
            })
            .collect();
        BandInputs {
            heights,
            w1: packed.iter().map(|p| self.upload(&p.0, "sprite bytes")).collect(),
            w2: packed.iter().map(|p| self.upload(&p.1, "class bytes")).collect(),
            painted_any: packed.iter().map(|p| p.2).collect(),
            depth: self.upload(&stamped.depth[a..b], "depth"),
            haze_a: self.upload(&stamped.haze_a[a..b], "haze cover"),
            grass_a: self.upload(&stamped.grass_a[a..b], "grass cover"),
            area: if clip_on { self.upload(&stamped.area_a[a..b], "area") } else { self.buffer(4, "area") },
        }
    }

    /// A band's inputs made on the GPU: trees stamped from the atlas, and depth, haze, grass and the drawn
    /// area's soft edge, like `engine::stamp_region`.
    #[allow(clippy::too_many_arguments)]
    fn gpu_inputs(&self, enc: &mut wgpu::CommandEncoder, g0: &wgpu::BindGroup, project: &Project, plan: &Plan, scale: f32, region: Region, y0: usize, y1: usize, shared: &Shared) -> BandInputs {
        let g = &project.gen;
        let s = scale * g.scale.clamp(0.1, 4.0);
        let (w, h) = (region.w, y1 - y0);
        let n = w * h;
        let atlas = shared.atlas.as_ref().expect("atlas for GPU stamping");
        let tmp = self.buffer(n, "scratch");
        let half = w.div_ceil(2) * h.div_ceil(2);
        let (h1, h2) = (self.buffer(half, "half a"), self.buffer(half, "half b"));

        // trees, layer by layer, in drawing order
        let (gy0, gy1) = ((region.y0 + y0) as i32, (region.y0 + y1) as i32);
        let (gx0, gx1) = (region.x0 as i32, (region.x0 + w) as i32);
        let win = self.buffer(n, "winner");
        let mut heights = Vec::new();
        let mut w1 = Vec::new();
        let mut w2 = Vec::new();
        let mut painted_any = Vec::new();
        for layer in &atlas.layers {
            let words: Vec<[u32; 16]> = layer
                .iter()
                .filter(|q| {
                    let (x0, yy0, size) = (q[0] as i32, q[1] as i32, q[3] as i32);
                    yy0 < gy1 && yy0 + size > gy0 && x0 < gx1 && x0 + size > gx0
                })
                .copied()
                .collect();
            painted_any.push(words.iter().any(|q| (q[6] >> 7) & 1 == 1));
            let (hz, a, b) = (self.buffer(n, "height"), self.buffer(n, "sprite bytes"), self.buffer(n, "class bytes"));
            enc.clear_buffer(&hz, 0, None);
            enc.clear_buffer(&a, 0, None);
            enc.clear_buffer(&b, 0, None);
            enc.clear_buffer(&win, 0, None);
            if !words.is_empty() {
                let inst = self.upload(&words, "trees");
                let count = words.len() as u32;
                let gx = count.min(65535);
                for entry in ["k_stamp_z", "k_stamp_win", "k_stamp_attr"] {
                    self.run_groups(enc, g0, entry, step_u([count, 0, 0, 0]), (gx, count.div_ceil(gx)), &[(0, &atlas.z)], &[(0, &atlas.w), (1, &atlas.c), (2, &inst), (3, &a), (4, &b), (5, &hz), (6, &win)]);
                }
            }
            heights.push(hz);
            w1.push(a);
            w2.push(b);
        }

        // depth, worked out every other pixel
        let fine = plan.fine;
        let (cw, ch) = ((w - 1) / fine + 2, (h - 1) / fine + 2);
        let coarse = self.buffer(cw * ch, "coarse depth");
        self.run(enc, g0, "k_depth_coarse", step_u([cw as u32, ch as u32, fine as u32, 0]), cw * ch, &[(0, &coarse)], &[], None);
        let depth = self.buffer(n, "depth");
        self.run(enc, g0, "k_upsample", step_u([cw as u32, ch as u32, fine as u32, 0]), n, &[(0, &coarse), (1, &depth)], &[], None);
        // haze and grass
        let haze_a = self.buffer(n, "haze cover");
        self.run(enc, g0, "k_haze", Step::default(), n, &[(0, &depth), (1, &haze_a)], &[], None);
        let imp = self.buffer(n, "blades");
        self.run(enc, g0, "k_grass_imp", Step::default(), n, &[(0, &depth), (1, &imp)], &[], None);
        let cov = self.buffer(n, "grass cover");
        self.blur(enc, g0, &imp, &cov, w, h, 1.2 * s, &tmp, &h1, &h2);
        let grass_a = self.buffer(n, "grass cover");
        self.run(enc, g0, "k_grass_a", Step::default(), n, &[(0, &cov), (1, &grass_a)], &[], None);
        // the drawn area, filled on the CPU and softened here
        let clip_on = g.contain && plan.any_shape && g.contain_clip;
        let area = if clip_on {
            let mut mask = crate::field::Field::new(w, h);
            let (ox, oy) = (region.x0 as f32, (region.y0 + y0) as f32);
            for shape in project.shapes_clearings_last() {
                let pts: Vec<[f32; 2]> = shape.points.iter().map(|p| [p[0] * scale - ox, p[1] * scale - oy]).collect();
                crate::engine::fill_polygon(&mut mask, &pts, if shape.op == ShapeOp::Add { 1.0 } else { 0.0 });
            }
            let m = self.upload(&mask.data, "area mask");
            let blurred = self.buffer(n, "area blur");
            self.blur(enc, g0, &m, &blurred, w, h, 0.8 * s, &tmp, &h1, &h2);
            let area = self.buffer(n, "area");
            self.run(enc, g0, "k_area", Step::default(), n, &[(0, &blurred), (1, &area)], &[], None);
            area
        } else {
            self.buffer(4, "area")
        };
        BandInputs { heights, w1, w2, painted_any, depth, haze_a, grass_a, area }
    }

    /// Shade rows `y0..y1` of a region from its inputs.
    #[allow(clippy::too_many_arguments)]
    fn shade_band(&self, enc: &mut wgpu::CommandEncoder, g0: &wgpu::BindGroup, project: &Project, plan: &Plan, scale: f32, region: Region, y0: usize, y1: usize, params: wgpu::Buffer, inp: BandInputs) -> GpuLayers {
        let g = &project.gen;
        let light = &project.light;
        let f = g.scale.clamp(0.1, 4.0);
        let s = scale * f;
        let (w, h) = (region.w, y1 - y0);
        let n = w * h;
        let realistic = g.edge_mode == EdgeMode::Realistic;
        let clip_on = g.contain && plan.any_shape && g.contain_clip;
        let ang = light.shadow_angle.to_radians();
        let slope = 0.6 / light.shadow_length.max(0.05);
        let fi = &g.fire;
        let glow_on = fi.amount > 0.0 && fi.glow > 0.0 && fi.flames > 0.0;
        let (gw, gh) = (w / 4 + 1, h / 4 + 1);
        // scratch
        let tmp = self.buffer(n, "scratch");
        let half = w.div_ceil(2) * h.div_ceil(2);
        let (h1, h2) = (self.buffer(half, "half a"), self.buffer(half, "half b"));

        // 6. top surface, 7. shadows
        let top = self.buffer(n, "top");
        self.run(enc, g0, "k_top", Step::default(), n, &[(0, &inp.grass_a), (1, &inp.heights[0]), (2, &inp.heights[1]), (3, &inp.heights[2]), (4, &inp.heights[3]), (5, &top)], &[], None);
        let occ_h = self.buffer(n, "occluder height");
        let occ_k = self.buffer(n, "occluder distance");
        if light.shadows {
            let (sx, sy) = (-ang.cos(), -ang.sin());
            let swap = sx.abs() > sy.abs();
            let (dx, dy) = if swap { (sy, sx) } else { (sx, sy) };
            let (minor, major) = if swap { (h, w) } else { (w, h) };
            let len = 1.0 / dy.abs().max(1e-6);
            // rows after which an occluder can no longer shade anything, and how far it drifts sideways meanwhile
            let ex = dx * len;
            let reach_rows = (plan.max_height() * s / (slope * len)).ceil() + 2.0;
            let margin = ((reach_rows * ex.abs()).ceil() as usize + 2).min(minor);
            let chunk = 256usize;
            let groups = minor.div_ceil(chunk);
            let rows = self.buffer(groups * 2 * (chunk + 2 * margin), "rows g");
            let rows_k = self.buffer(groups * 2 * (chunk + 2 * margin), "rows k");
            let st = Step { f: [ex, slope * len, len, 0.0], u: [swap as u32, (dy > 0.0) as u32, minor as u32, major as u32], t: [chunk as u32, margin as u32, 0, 0], ..Default::default() };
            self.run_groups(enc, g0, "k_horizon", st, (groups as u32, 1), &[(0, &top), (1, &occ_h), (2, &occ_k), (3, &rows), (4, &rows_k)], &[]);
        }
        let shadow_radius = 2.5 * s * light.shadow_softness * (0.7 + 0.3 * light.shadow_length);
        let raw = self.buffer(n, "raw shadow");
        // receiver: (mode, buffer); presence: optional mask
        let shadow_for = |enc: &mut wgpu::CommandEncoder, mode: u32, recv: Option<&wgpu::Buffer>, present: Option<&wgpu::Buffer>| -> Option<wgpu::Buffer> {
            if !light.shadows {
                return None;
            }
            let mut f: Vec<(usize, &wgpu::Buffer)> = vec![(0, &occ_h), (1, &occ_k), (4, &raw)];
            if let Some(r) = recv {
                f.push((2, r));
            }
            if let Some(m) = present {
                f.push((3, m));
            }
            self.run(enc, g0, "k_shadow_raw", step_u([mode, present.is_some() as u32, 0, 0]), n, &f, &[], None);
            let out = self.buffer(n, "shadow");
            self.blur(enc, g0, &raw, &out, w, h, shadow_radius, &tmp, &h1, &h2);
            if g.contain_shadows && clip_on {
                self.run(enc, g0, "k_clip", step_u([1, 0, 0, 0]), n, &[(0, &out), (1, &inp.area), (2, &tmp)], &[], None);
                enc.copy_buffer_to_buffer(&tmp, 0, &out, 0, (n * 4) as u64);
            }
            Some(out)
        };

        // glow around flames, on a coarse grid
        let glow = self.buffer(gw * gh, "glow");
        if glow_on {
            let src = self.buffer(gw * gh, "glow source");
            self.run(enc, g0, "k_glow_src", step_u([gw as u32, gh as u32, 4, 0]), gw * gh, &[(0, &src)], &[], None);
            let gtmp = self.buffer(gw * gh, "glow scratch");
            let (gh1, gh2) = (self.buffer(gw.div_ceil(2) * gh.div_ceil(2), "glow half"), self.buffer(gw.div_ceil(2) * gh.div_ceil(2), "glow half"));
            self.blur(enc, g0, &src, &glow, gw, gh, 18.0 * s / 4.0, &gtmp, &gh1, &gh2);
        }

        // haze and ground shadow
        let haze = self.buffer(n, "haze");
        self.run(enc, g0, "k_clip", step_u([clip_on as u32, 0, 0, 0]), n, &[(0, &inp.haze_a), (1, &inp.area), (2, &haze)], &[], None);
        let ground = if realistic { shadow_for(enc, 0, None, None) } else { None };

        let smooth_r = crate::smooth::radius(g.depth.smooth, 0.5 * (g.crown_min + g.crown_max) * s);
        // grass
        let mut colors = Vec::new();
        let mut shadows = Vec::new();
        let grass_col = self.buffer(2 * n, "grass colour");
        self.run(enc, g0, "k_grass", Step::default(), n, &[(0, &inp.grass_a), (7, &glow), (8, &inp.depth), (9, &inp.area)], &[(2, &grass_col)], None);
        let grass_col = self.smooth_colours(enc, g0, grass_col, n, smooth_r);
        colors.push(grass_col);
        shadows.push(if realistic { shadow_for(enc, 2, Some(&inp.grass_a), Some(&inp.grass_a)) } else { None });

        // tree layers
        let recols = [[0.85, 0.95, 0.8], [1.0, 1.0, 1.0], [1.05, 1.1, 0.95], [0.8, 0.9, 0.75]];
        let (blurred, b3, b9, b_big, alpha, painted_blur, mask, painted) =
            (self.buffer(n, "b5"), self.buffer(n, "b3"), self.buffer(n, "b9"), self.buffer(n, "big"), self.buffer(n, "alpha"), self.buffer(n, "painted blur"), self.buffer(n, "mask"), self.buffer(n, "painted"));
        for j in 0..inp.heights.len() {
            let hb = &inp.heights[j];
            self.blur(enc, g0, hb, &blurred, w, h, 5.0 * s, &tmp, &h1, &h2);
            self.blur(enc, g0, hb, &b3, w, h, 3.0 * s, &tmp, &h1, &h2);
            self.blur(enc, g0, hb, &b9, w, h, 9.0 * s, &tmp, &h1, &h2);
            self.blur(enc, g0, hb, &b_big, w, h, g.depth.shade_radius.max(2.0) * s, &tmp, &h1, &h2);
            self.run(enc, g0, "k_mask", Step::default(), n, &[(0, hb), (1, &mask), (2, &painted)], &[(0, &inp.w2[j])], None);
            self.blur(enc, g0, &mask, &alpha, w, h, 0.6 * s, &tmp, &h1, &h2);
            if inp.painted_any[j] {
                self.blur(enc, g0, &painted, &painted_blur, w, h, 1.0 * s, &tmp, &h1, &h2);
            }
            let col = self.buffer(2 * n, "layer colour");
            let r = recols[j];
            let st = Step { f: [r[0], r[1], r[2], 0.0], u: [inp.painted_any[j] as u32, 0, 0, 0], ..Default::default() };
            self.run(
                enc,
                g0,
                "k_shade_tree",
                st,
                n,
                &[(0, hb), (1, &blurred), (2, &b3), (3, &b9), (4, &b_big), (5, &alpha), (6, &painted_blur), (7, &glow), (8, &inp.depth), (9, &inp.area)],
                &[(0, &inp.w1[j]), (1, &inp.w2[j]), (2, &col)],
                None,
            );
            let col = self.smooth_colours(enc, g0, col, n, smooth_r);
            colors.push(col);
            shadows.push(shadow_for(enc, 1, Some(hb), Some(&alpha)));
        }

        GpuLayers {
            w,
            h,
            scale,
            tree_counts: plan.tree_counts,
            fit_inset: plan.fit_inset,
            trees: Vec::new(),
            params,
            haze,
            haze_color: g.haze_color,
            ground,
            colors,
            shadows,
        }
    }

    /// Canopy smoothing of one layer's packed colours, or the colours as they are when it is off.
    fn smooth_colours(&self, enc: &mut wgpu::CommandEncoder, g0: &wgpu::BindGroup, col: wgpu::Buffer, n: usize, r: usize) -> wgpu::Buffer {
        if r == 0 {
            return col;
        }
        let out = self.buffer(2 * n, "smoothed colour");
        self.run(enc, g0, "k_smooth", step_u([r as u32, 0, 0, 0]), n, &[], &[(0, &col), (1, &out)], None);
        out
    }

    /// The grading block for `k_compose`.
    fn grades(layers: &GpuLayers, grading: &Grading, opts: ComposeOptions) -> Vec<[f32; 4]> {
        let mut v = vec![[0f32; 4]; 40];
        let default = LayerGrade::default();
        let mut put = |k: usize, gr: &LayerGrade| {
            let on = gr.hue.abs() > 0.01;
            let a = gr.hue.to_radians();
            v[3 * k] = [gr.visible as u8 as f32, gr.opacity, on as u8 as f32, a.cos()];
            v[3 * k + 1] = [a.sin(), gr.saturation, gr.brightness, gr.contrast];
            v[3 * k + 2] = [gr.tint[0], gr.tint[1], gr.tint[2], gr.receive_shadow as u8 as f32];
        };
        for k in 0..7 {
            put(k, grading.layers.get(k).unwrap_or(&default));
        }
        put(7, &grading.global);
        v[24] = [opts.shadows as u8 as f32, opts.shadow_only as u8 as f32, opts.background.is_some() as u8 as f32, opts.premultiplied as u8 as f32];
        let sc = grading.shadow_color;
        v[25] = [sc[0], sc[1], sc[2], grading.shadow_opacity];
        let has = |k: usize| layers.shadows[k].is_some() as u8 as f32;
        v[26] = [has(0), has(1), has(2), has(3)];
        v[27] = [has(4), 0.0, 0.0, 0.0];
        let bg = opts.background.unwrap_or([0.0; 3]);
        v[28] = [bg[0], bg[1], bg[2], 1.0];
        v[29] = [layers.haze_color[0], layers.haze_color[1], layers.haze_color[2], 0.0];
        v[30] = [layers.ground.is_some() as u8 as f32, 0.0, 0.0, 0.0];
        v
    }

    fn compose_to(&self, layers: &GpuLayers, grading: &Grading, opts: ComposeOptions, view: &wgpu::TextureView) {
        let grades = self.uniform(&Self::grades(layers, grading, opts));
        let empty = self.buffer(4, "none");
        let g0 = self.group0_with(&layers.params, &grades, [&empty, &empty, &empty, &empty, &empty]);
        let mut enc = self.device.create_command_encoder(&Default::default());
        // missing shadows stay on their placeholders; the grading block says they are absent
        let mut f: Vec<(usize, &wgpu::Buffer)> = vec![(0, &layers.haze)];
        if let Some(gs) = &layers.ground {
            f.push((1, gs));
        }
        for (k, sh) in layers.shadows.iter().enumerate() {
            if let Some(b) = sh {
                f.push((2 + k, b));
            }
        }
        let c = &layers.colors;
        self.run(&mut enc, &g0, "k_compose", Step::default(), layers.w * layers.h, &f, &[(0, &c[0]), (1, &c[1]), (2, &c[2]), (3, &c[3]), (4, &c[4])], Some(view));
        self.queue.submit([enc.finish()]);
    }

    fn image(&self, w: usize, h: usize) -> GpuImage {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("forest image"),
            size: wgpu::Extent3d { width: w as u32, height: h as u32, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        // egui reads texture values as already gamma encoded, so the view must not decode them again
        let view = texture.create_view(&wgpu::TextureViewDescriptor { usage: Some(wgpu::TextureUsages::TEXTURE_BINDING), ..Default::default() });
        GpuImage { texture, view, w, h }
    }

    /// Composite into `target`, made or remade to fit, for showing on screen.
    pub fn compose_image(&self, layers: &GpuLayers, grading: &Grading, opts: ComposeOptions, target: &mut Option<GpuImage>) {
        if !matches!(target, Some(t) if t.w == layers.w && t.h == layers.h) {
            *target = Some(self.image(layers.w, layers.h));
        }
        let t = target.as_ref().unwrap();
        let storage_view = t.texture.create_view(&Default::default());
        self.compose_to(layers, grading, opts, &storage_view);
    }

    /// Composite and read back straight-alpha RGBA8, like `compose::compose`.
    pub fn compose_rgba(&self, layers: &GpuLayers, grading: &Grading, opts: ComposeOptions) -> Vec<u8> {
        let img = self.image(layers.w, layers.h);
        let storage_view = img.texture.create_view(&Default::default());
        self.compose_to(layers, grading, opts, &storage_view);
        let (w, h) = (layers.w as u32, layers.h as u32);
        let row = (w * 4).div_ceil(256) * 256;
        let read = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (row * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &img.texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo { buffer: &read, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.queue.submit([enc.finish()]);
        let slice = read.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        let data = slice.get_mapped_range().expect("mapped readback");
        let mut out = vec![0u8; (w * h * 4) as usize];
        for y in 0..h as usize {
            let src = &data[y * row as usize..y * row as usize + (w * 4) as usize];
            out[y * (w * 4) as usize..(y + 1) * (w * 4) as usize].copy_from_slice(src);
        }
        drop(data);
        read.unmap();
        out
    }

    /// Wait for queued GPU work, for timing.
    pub fn wait(&self) {
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
    }
}
