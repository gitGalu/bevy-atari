pub mod antic;
pub mod gtia;
mod render_resources;
mod palette;
pub mod pia;
pub mod pokey;
mod system;
mod atari800_state;
mod js_api;
use wasm_bindgen::prelude::*;
use antic::ModeLineDescr;
use bevy::reflect::TypeUuid;
use bevy::{
    prelude::*,
    render::{
        camera::{OrthographicProjection, WindowOrigin},
        entity::Camera2dBundle,
        mesh::shape,
        pass::ClearColor,
        pipeline::{CullMode, PipelineDescriptor, RenderPipeline},
        render_graph::{base, AssetRenderResourcesNode, RenderGraph, RenderResourcesNode},
        renderer::RenderResources,
        shader::{ShaderStage, ShaderStages},
    },
};
use system::{AtariSystem, W65C02S};
use render_resources::{Charset, LineData, Palette, GTIAColors};



const SCAN_LINE_CYCLES: usize = 114;
const PAL_SCAN_LINES: usize = 312;
const NTSC_SCAN_LINES: usize = 262;

const MAX_SCAN_LINES: usize = NTSC_SCAN_LINES;

const VERTEX_SHADER: &str = include_str!("shaders/antic.vert");
const FRAGMENT_SHADER: &str = include_str!("shaders/antic.frag");

#[derive(RenderResources, TypeUuid)]
#[uuid = "1e08866c-0b8a-437e-8bce-37733b25127e"]
struct AnticLine {
    pub line_width: u32,
    pub mode: u32,
    pub hscrol: u32,
    pub data: LineData,
    pub gtia_colors: GTIAColors,
    pub charset: Charset,
}
#[derive(RenderResources, Default, TypeUuid)]
#[uuid = "f145d910-99c5-4df5-b673-e822b1389222"]
struct AtariPalette {
    pub palette: Palette,
}

#[derive(Debug, Default)]
struct PerfMetrics {
    frame_cnt: usize,
    cpu_cycle_cnt: usize,
}

#[derive(Default)]
struct AnticResources {
    pipeline_handle: Handle<PipelineDescriptor>,
    palette_handle: Handle<AtariPalette>,
    mesh_handle: Handle<Mesh>,
}
fn create_mode_line(
    commands: &mut Commands,
    resources: &AnticResources,
    mode_line: ModeLineDescr,
    antic_line_nr: usize,
    system: &AtariSystem,
) {
    if mode_line.n_bytes == 0 || mode_line.width == 0 || mode_line.height == 0 {
        return;
    }
    let line_data = LineData::new(&system.ram[mode_line.data_offset..mode_line.data_offset + 48]);
    let gtia_colors = system.gtia.get_colors();

    let charset_offset = (mode_line.chbase as usize) * 256;
    // let charset = &system.ram[charset_offset..charset_offset + 1024]; // TODO - 512 byte charsets?

    let charset = Charset::new(&system.ram[charset_offset..charset_offset + 1024]);

    commands
        .spawn(MeshBundle {
            mesh: resources.mesh_handle.clone(),
            render_pipelines: RenderPipelines::from_pipelines(vec![RenderPipeline::new(
                resources.pipeline_handle.clone_weak(),
            )]),
            transform: Transform::from_translation(Vec3::new(
                0.0,
                120.0 - (mode_line.scan_line as f32) - mode_line.height as f32 / 2.0 - antic_line_nr as f32,
                0.0,
            ))
            .mul_transform(Transform::from_scale(Vec3::new(
                mode_line.width as f32,
                mode_line.height as f32,
                1.0,
            ))),
            ..Default::default()
        })
        .with(AnticLine {
            // chbase: mode_line.chbase as u32,
            mode: mode_line.mode as u32,
            gtia_colors,
            line_width: mode_line.width as u32,
            hscrol: mode_line.hscrol as u32,
            data: line_data,
            charset: charset,
        })
        .with(resources.palette_handle.clone_weak());
}

#[derive(Default)]
struct Debugger {
    enabled: bool,
    instr_cnt: usize,
}

fn atari_system(
    commands: &mut Commands,
    keyboard: Res<Input<KeyCode>>,
    antic_resources: ResMut<AnticResources>,
    antic_lines: Query<(Entity, &AnticLine)>,
    // mut charsets: ResMut<Assets<AnticCharset>>,
    mut debug: Local<Debugger>,
    mut cpu: ResMut<W65C02S>,
    mut atari_system: ResMut<AtariSystem>,
    mut perf_metrics: Local<PerfMetrics>,
) {
    // if perf_metrics.frame_cnt > 120 {
    //     return;
    // }
    {
        let mut guard = js_api::ARRAY.write();
        for event in guard.drain(..) {
            atari_system.set_joystick(0, event.up, event.down, event.left, event.right, event.fire);
        }

    }
    atari_system.handle_keyboard(&keyboard);
    for (entity, _) in antic_lines.iter() {
        commands.despawn(entity);
    }
    let mut vblank = false;
    let mut next_scan_line: usize = 8;
    let mut dli_scan_line: usize = 0xffff;

    // let charset: Vec<_> = atari_system.ram
    //     .iter()
    //     .cloned()
    //     .collect();
    // charsets.set(&antic_resources.charset_handle, AnticCharset { charset });
    let mut antic_line_nr = 0;
    for scan_line in 0..MAX_SCAN_LINES {
        // info!("scan_line: {}", scan_line);
        atari_system.antic.scan_line = scan_line;

        vblank = vblank || scan_line >= 248;
        if !vblank && next_scan_line == scan_line {
            let dlist = atari_system.antic.dlist();
            let mut dlist_data: [u8; 3] = [0; 3];
            dlist_data.copy_from_slice(&atari_system.ram[dlist..dlist + 3]);
            if let Some(mode_line) = atari_system.antic.create_next_mode_line(&dlist_data) {
                next_scan_line = scan_line + mode_line.height;
                if mode_line.dli {
                    dli_scan_line = next_scan_line - 1;
                }
                // info!("antic line: {:?}, next_scan_line: {:?}", mode_line, next_scan_line);
                create_mode_line(commands, &antic_resources, mode_line, antic_line_nr, &atari_system);
                // antic_line_nr += 1;
            } else {
                vblank = true;
            }
        }
        for n in 0..SCAN_LINE_CYCLES {
            if n < 2 {
                if scan_line == 0 {
                    let iter = keyboard.get_just_pressed().next();
                    if iter.is_some() {
                        info!("kbd int!");
                        cpu.set_irq(n == 0);
                    }
                } else if scan_line == dli_scan_line {
                    // bevy::log::info!("DLI, scanline: {}", scan_line);
                    atari_system.antic.set_dli();
                    cpu.set_nmi(n == 0);
                } else if scan_line == 248 {
                    // bevy::log::info!("VBI, scanline: {}", scan_line);
                    atari_system.antic.set_vbi();
                    cpu.set_nmi(n == 0);
                }
            }
            let pc = cpu.get_pc() as usize;
            // if pc == 0xc2b3 {
            //     debug.enabled = true;
            // }
            if debug.enabled {
                if debug.instr_cnt < 200 {
                    if let Ok(inst) = disasm6502::from_array(&atari_system.ram[pc..pc + 16]) {
                        if let Some(i) = inst.get(0) {
                            info!("{:04x?}: {} {:?}", pc, i, *cpu);
                        }
                    }
                } else {
                    panic!("STOP");
                }
                debug.instr_cnt += 1;
            }
            cpu.step(&mut *atari_system);
            perf_metrics.cpu_cycle_cnt += 1;
        }

    }
    perf_metrics.frame_cnt += 1;
}

fn setup(
    commands: &mut Commands,
    mut atari_system: ResMut<AtariSystem>,
    mut cpu: ResMut<W65C02S>,
    mut antic_resources: ResMut<AnticResources>,
    mut pipelines: ResMut<Assets<PipelineDescriptor>>,
    mut shaders: ResMut<Assets<Shader>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut palettes: ResMut<Assets<AtariPalette>>,
    mut render_graph: ResMut<RenderGraph>,
) {
    // let atari800_state = atari800_state::load_state(include_bytes!("../ls.state.dat"));
    // let atari800_state = atari800_state::load_state(include_bytes!("../fred.state.dat"));
    let atari800_state = atari800_state::load_state(include_bytes!("../robbo.state.dat"));
    // let atari800_state = atari800_state::load_state(include_bytes!("../basic.state.dat"));
    atari_system.ram.copy_from_slice(atari800_state.memory.data);
    let gtia = atari800_state.gtia;
    let antic = atari800_state.antic;

    atari_system.gtia.write(gtia::COLBK, gtia.colbk);
    atari_system.gtia.write(gtia::COLPF0, gtia.colpf0);
    atari_system.gtia.write(gtia::COLPF1, gtia.colpf1);
    atari_system.gtia.write(gtia::COLPF2, gtia.colpf2);
    atari_system.gtia.write(gtia::COLPF3, gtia.colpf3);

    atari_system.antic.write(antic::DMACTL, antic.dmactl);
    atari_system.antic.write(antic::CHACTL, antic.chactl);
    atari_system.antic.write(antic::CHBASE, antic.chbase);
    atari_system.antic.write(antic::DLIST, (antic.dlist & 0xff) as u8);
    atari_system
        .antic
        .write(antic::DLIST + 1, (antic.dlist >> 8) as u8);
    atari_system.antic.write(antic::NMIEN, antic.nmien);
    atari_system.antic.write(antic::NMIST, antic.nmist);
    atari_system.antic.write(antic::PMBASE, antic.pmbase);

    cpu.step(&mut *atari_system); // changes state into Running
    cpu.set_pc(atari800_state.cpu.pc);
    cpu.set_a(atari800_state.cpu.reg_a);
    cpu.set_x(atari800_state.cpu.reg_x);
    cpu.set_y(atari800_state.cpu.reg_y);
    cpu.set_p(atari800_state.cpu.reg_p);
    cpu.set_s(atari800_state.cpu.reg_s);

    let mut pipeline_descr = PipelineDescriptor::default_config(ShaderStages {
        vertex: shaders.add(Shader::from_glsl(ShaderStage::Vertex, VERTEX_SHADER)),
        fragment: Some(shaders.add(Shader::from_glsl(ShaderStage::Fragment, FRAGMENT_SHADER))),
    });
    if let Some(descr) = pipeline_descr.rasterization_state.as_mut() {
        descr.cull_mode = CullMode::None;
    }

    // Create a new shader pipeline
    antic_resources.pipeline_handle = pipelines.add(pipeline_descr);

    // Add an AssetRenderResourcesNode to our Render Graph. This will bind AnticCharset resources to our shader
    render_graph.add_system_node(
        "atari_palette",
        AssetRenderResourcesNode::<AtariPalette>::new(false),
    );
    render_graph
        .add_node_edge("atari_palette", base::node::MAIN_PASS)
        .unwrap();

    // Add an AssetRenderResourcesNode to our Render Graph. This will bind AnticLine resources to our shader
    render_graph.add_system_node("antic_line", RenderResourcesNode::<AnticLine>::new(true));

    // Add a Render Graph edge connecting our new "antic_line" node to the main pass node. This ensures "antic_line" runs before the main pass
    render_graph
        .add_node_edge("antic_line", base::node::MAIN_PASS)
        .unwrap();

    antic_resources.palette_handle = palettes.add(AtariPalette::default());
    antic_resources.mesh_handle = meshes.add(Mesh::from(shape::Quad {
        size: Vec2::new(1.0, 1.0),
        flip: false,
    }));

    // Setup our world
    // commands.spawn(Camera3dBundle {
    //     transform: Transform::from_translation(Vec3::new(-10.0 * 8.0, 0.0 * 8.0, 40.0 * 8.0))
    //         .looking_at(Vec3::new(-2.0 * 8.0, -0.0 * 8.0, 0.0), Vec3::unit_y()),
    //     ..Default::default()
    // });

    commands.spawn(Camera2dBundle {
        orthographic_projection: OrthographicProjection {
            bottom: 0.0,
            top: 2.0 * 240.0,
            left: 0.0,
            right: 2.0 * 384.0,
            window_origin: WindowOrigin::Center,
            ..Default::default()
        },
        transform: Transform::from_translation(Vec3::default())
            .mul_transform(Transform::from_scale(Vec3::new(1.0 / 3.0, 1.0 / 3.0, 1.0))),
        ..Default::default()
    });
}

/// This example illustrates how to create a custom material asset and a shader that uses that material
fn main() {
    let mut app = App::build();
    app.add_resource(WindowDescriptor {
        title: "Robbo".to_string(),
        resizable: true,
        mode: bevy::window::WindowMode::Windowed,
        #[cfg(target_arch = "wasm32")]
        canvas: Some("#bevy-canvas".to_string()),
        vsync: true,
        ..Default::default()
    });
    app.add_plugins(DefaultPlugins);

    #[cfg(target_arch = "wasm32")]
    app.add_plugin(bevy_webgl2::WebGL2Plugin);
    app.add_asset::<AnticLine>()
        .add_asset::<AtariPalette>()
        .add_asset::<StandardMaterial>()
        .add_resource(ClearColor(gtia::atari_color(0)))
        .add_resource(AtariSystem::new())
        .add_resource(W65C02S::new())
        .add_resource(AnticResources::default())
        .add_startup_system(setup)
        .add_system(atari_system)
        .run();
}
