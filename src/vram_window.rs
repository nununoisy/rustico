extern crate sdl2;

use rusticnes_core::mmc::mapper::Mapper;
use rusticnes_core::nes::NesState;
use rusticnes_core::ppu;
use rusticnes_core::palettes::NTSC_PAL;

use sdl2::event::Event;
use sdl2::event::WindowEvent;
use sdl2::pixels::Color;
use sdl2::pixels::PixelFormatEnum;
use sdl2::rect::Rect;
use sdl2::render::TextureAccess;

use drawing::SimpleBuffer;

pub struct VramWindow {
  pub canvas: sdl2::render::WindowCanvas,
  pub chr0_buffer: SimpleBuffer,
  pub chr1_buffer: SimpleBuffer,
  pub nametable_buffer: SimpleBuffer,
  pub shown: bool,
}

impl VramWindow {
  pub fn new(sdl_context: &sdl2::Sdl) -> VramWindow {
    let video_subsystem = sdl_context.video().unwrap();

    let window = video_subsystem.window("VRAM Debugger", 512, 736)
        .position(570, 50)
        .hidden()
        .opengl()
        .build()
        .unwrap();

    let mut canvas = window.into_canvas().present_vsync().build().unwrap();
    canvas.set_draw_color(Color::RGB(0, 0, 0));
    canvas.clear();
    canvas.present();

    return VramWindow {
      canvas: canvas,
      chr0_buffer: SimpleBuffer::new(128, 128),
      chr1_buffer: SimpleBuffer::new(128, 128),
      nametable_buffer: SimpleBuffer::new(512, 480),
      shown: false,
    }
  }

  pub fn generate_chr_pattern(mapper: &mut Mapper, pattern_address: u16, buffer: &mut SimpleBuffer) {
    let debug_pallete: [u8; 4] = [255, 192, 128, 0];
    for x in 0 .. 16 {
      for y in 0 .. 16 {
        let tile = y * 16 + x;
        for px in 0 .. 8 {
          for py in 0 .. 8 {
            let palette_index = ppu::decode_chr_pixel(mapper, pattern_address, tile as u8, px as u8, py as u8);
            buffer.put_pixel(x * 8 + px, y * 8 + py, &[
              debug_pallete[palette_index as usize],
              debug_pallete[palette_index as usize],
              debug_pallete[palette_index as usize],
              255]);
          }
        }
      }
    }
  }

  pub fn generate_nametables(mapper: &mut Mapper, ppu: &mut ppu::PpuState, buffer: &mut SimpleBuffer) {
    let mut pattern_address = 0x0000;
    if (ppu.control & 0x10) != 0 {
      pattern_address = 0x1000;
    }
    for tx in 0 .. 64 {
      for ty in 0 .. 60 {
        let tile_index = ppu.get_bg_tile(mapper, tx, ty);
        let palette_index = ppu.get_bg_palette(mapper, tx, ty);
        for px in 0 .. 8 {
          for py in 0 .. 8 {
            let bg_index = ppu::decode_chr_pixel(mapper, pattern_address, tile_index as u8, px as u8, py as u8);
            let mut palette_color = ppu.passively_read_byte(mapper, ((palette_index << 2) + bg_index) as u16 + 0x3F00) as usize * 3;
            if bg_index == 0 {
              palette_color = ppu.passively_read_byte(mapper, bg_index as u16 + 0x3F00) as usize * 3;
            }
            buffer.put_pixel(tx as u32 * 8 + px as u32, ty as u32 * 8 + py as u32, &[
              NTSC_PAL[palette_color + 0],
              NTSC_PAL[palette_color + 1],
              NTSC_PAL[palette_color + 2],
              255]);
          }
        }
      }
    }
  }

  pub fn update(&mut self, nes: &mut NesState) {
    VramWindow::generate_nametables(&mut *nes.mapper, &mut nes.ppu, &mut self.nametable_buffer);
    VramWindow::generate_chr_pattern(&mut *nes.mapper, 0x0000, &mut self.chr0_buffer);
    VramWindow::generate_chr_pattern(&mut *nes.mapper, 0x1000, &mut self.chr1_buffer);
  }
  

  pub fn draw(&mut self) {
    let texture_creator = self.canvas.texture_creator();
    let mut nametable_texture = texture_creator.create_texture(PixelFormatEnum::ABGR8888, TextureAccess::Streaming, 512, 480).unwrap();
    let mut chr_0_texture = texture_creator.create_texture(PixelFormatEnum::ABGR8888, TextureAccess::Streaming, 128, 128).unwrap();
    let mut chr_1_texture = texture_creator.create_texture(PixelFormatEnum::ABGR8888, TextureAccess::Streaming, 128, 128).unwrap();
      
    self.canvas.set_draw_color(Color::RGB(255, 255, 255));
    let _ = nametable_texture.update(None, &self.nametable_buffer.buffer, 512 * 4);
    let _ = chr_0_texture.update(None, &self.chr0_buffer.buffer, 128 * 4);
    let _ = chr_1_texture.update(None, &self.chr1_buffer.buffer, 128 * 4);
    let _ = self.canvas.copy(&chr_0_texture    , None, Rect::new(  0,   0, 256, 256));
    let _ = self.canvas.copy(&chr_1_texture    , None, Rect::new(256,   0, 256, 256));
    let _ = self.canvas.copy(&nametable_texture, None, Rect::new(  0, 256, 512, 480));

    self.canvas.present();
  }

  pub fn handle_event(&mut self, _: &mut NesState, event: &sdl2::event::Event) {
    let self_id = self.canvas.window().id();
    match *event {
      Event::Window { window_id: id, win_event: WindowEvent::Close, .. } if id == self_id => {
        self.shown = false;
        self.canvas.window_mut().hide();
      },
      _ => ()
    }
  }
}

