use crate::gtia::atari_color;
use bevy::asset::Handle;
use bevy::core::{Byteable, Bytes};
use bevy::prelude::Color;
use bevy::render::{
    impl_render_resource_bytes,
    renderer::{RenderResource, RenderResourceType},
    texture::Texture,
};
use std::convert::TryInto;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Charset {
    pub data: [u8; 1024],
}

impl Charset {
    pub fn new(src: &[u8]) -> Self {
        Self {
            data: src.try_into().expect("byte slice of length 1024"),
        }
    }
}

unsafe impl Byteable for Charset {}
impl_render_resource_bytes!(Charset);

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LineData {
    pub data: [u8; 48],
    pub player0: [u8; 16],
    pub player1: [u8; 16],
    pub player2: [u8; 16],
    pub player3: [u8; 16],
}

impl LineData {
    pub fn new(src: &[u8], player0: &[u8], player1: &[u8], player2: &[u8], player3: &[u8]) -> Self {
        Self {
            data: src.try_into().expect("byte slice of length 48"),
            player0: player0.try_into().expect("slice of 16 bytes"),
            player1: player1.try_into().expect("slice of 16 bytes"),
            player2: player2.try_into().expect("slice of 16 bytes"),
            player3: player3.try_into().expect("slice of 16 bytes"),
        }
    }
}

unsafe impl Byteable for LineData {}
impl_render_resource_bytes!(LineData);

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub data: [Color; 256],
}

impl Default for Palette {
    fn default() -> Self {
        let palette: Vec<_> = (0..=255).map(|index| atari_color(index)).collect();
        Self {
            data: palette
                .as_slice()
                .try_into()
                .expect("byte slice of length 256"),
        }
    }
}

unsafe impl Byteable for Palette {}
impl_render_resource_bytes!(Palette);

#[repr(C)]
#[derive(Default, Clone, Copy, Debug)]
pub struct GTIAColors {
    pub regs: [[u32; 4]; 3],
    pub player_pos: [f32; 4],
    pub player_size: [f32; 4],
    pub prior: u32,
}

impl GTIAColors {
    pub fn new(
        colbk: u8,
        colpf0: u8,
        colpf1: u8,
        colpf2: u8,
        colpf3: u8,
        colpm0: u8,
        colpm1: u8,
        colpm2: u8,
        colpm3: u8,
        hposp0: u8,
        hposp1: u8,
        hposp2: u8,
        hposp3: u8,
        sizep0: u8,
        sizep1: u8,
        sizep2: u8,
        sizep3: u8,
        prior: u8,
    ) -> Self {
        Self {
            regs: [
                [colbk as u32, colpf0 as u32, colpf1 as u32, colpf2 as u32],
                [colbk as u32, colpf0 as u32, colpf1 as u32, colpf3 as u32],
                [colpm0 as u32, colpm1 as u32, colpm2 as u32, colpm3 as u32],
            ],
            player_pos: [hposp0 as f32, hposp1 as f32, hposp2 as f32, hposp3 as f32],
            player_size: [
                player_size(sizep0),
                player_size(sizep1),
                player_size(sizep2),
                player_size(sizep3),
            ],
            prior: prior as u32,
        }
    }
}

fn player_size(sizep: u8) -> f32 {
    match sizep & 3 {
        1 => 32.0,
        3 => 64.0,
        _ => 16.0,
    }
}

unsafe impl Byteable for GTIAColors {}
impl_render_resource_bytes!(GTIAColors);
