use application::RuntimeState;
use drawing;
use drawing::Color;
use drawing::SimpleBuffer;
use events::Event;
use panel::Panel;

use regex::Regex;

use rusticnes_core::apu::ApuState;
use rusticnes_core::apu::AudioChannelState;
use rusticnes_core::apu::PlaybackRate;
use rusticnes_core::apu::RingBuffer;
use rusticnes_core::apu::Timbre;
use rusticnes_core::mmc::mapper::Mapper;

use std::collections::VecDeque;
use std::collections::hash_map::HashMap;

#[derive(Clone, Copy, PartialEq)]
pub enum NoteType {
    Frequency,
    Noise,
    Waveform
}

#[derive(Clone, Copy, PartialEq)]
pub enum ScrollDirection {
    RightToLeft,
    LeftToRight,
    TopToBottom,
    BottomToTop,
    PlayerPiano
}

#[derive(Clone, Copy, PartialEq)]
pub enum KeySize {
    Small,
    Medium,
    Large
}

#[derive(Clone, Copy, PartialEq)]
pub enum PollingType {
    PpuFrame,
    PpuScanline,
    ApuQuarterFrame,
    ApuHalfFrame,
}

pub struct ChannelSlice {
    pub visible: bool,
    pub y: f32,
    pub thickness: f32,
    pub color: Color,
    pub note_type: NoteType,

}

impl ChannelSlice {
    fn none() -> ChannelSlice {
        return ChannelSlice{
            visible: false,
            y: 0.0,
            thickness: 0.0,
            color: Color::rgb(0,0,0),
            note_type: NoteType::Frequency,
        };
    }
}


fn draw_right_white_key_horiz(canvas: &mut SimpleBuffer, x: u32, y: u32, color: Color) {
    drawing::blend_rect(canvas, x + 8, y + 1, 8, 1, color);
    drawing::blend_rect(canvas, x + 1, y,    15, 1, color);
}

fn draw_center_white_key_horiz(canvas: &mut SimpleBuffer, x: u32, y: u32, color: Color) {
    drawing::blend_rect(canvas, x + 1, y,    15, 1, color);
    drawing::blend_rect(canvas, x + 8, y - 1, 8, 1, color);
    drawing::blend_rect(canvas, x + 8, y + 1, 8, 1, color);
}

fn draw_left_white_key_horiz(canvas: &mut SimpleBuffer, x: u32, y: u32, color: Color) {
    drawing::blend_rect(canvas, x + 8, y - 1, 8, 1, color);
    drawing::blend_rect(canvas, x + 1, y,    15, 1, color);
}

fn draw_black_key_horiz(canvas: &mut SimpleBuffer, x: u32, y: u32, color: Color) {
    drawing::blend_rect(canvas, x + 1, y - 1, 7, 1, color);
    drawing::blend_rect(canvas, x + 1, y,     7, 1, color);
    drawing::blend_rect(canvas, x + 1, y + 1, 7, 1, color);
}

fn draw_speaker_key_horiz(canvas: &mut SimpleBuffer, color: Color, x: u32, y: u32) {
    drawing::blend_rect(canvas, x +  2, y + 6 - 8, 3, 5, color);
    drawing::blend_rect(canvas, x +  5, y + 5 - 8, 1, 7, color);
    drawing::blend_rect(canvas, x +  6, y + 4 - 8, 1, 9, color);
    drawing::blend_rect(canvas, x +  7, y + 3 - 8, 1, 11, color);
    drawing::blend_rect(canvas, x +  8, y + 2 - 8, 1, 13, color);
    drawing::blend_rect(canvas, x + 10, y + 6 - 8, 1, 5, color);
    drawing::blend_rect(canvas, x + 12, y + 4 - 8, 1, 9, color);
}

// various utility functions for key drawing. The 1px offsets generally account for the
// 1px border along the top and between keys.
fn full_key_length(base_key_length: u32) -> u32 {
    return base_key_length - 1;
}

fn upper_key_length(base_key_length: u32) -> u32 {
    return base_key_length / 2;
}

fn lower_key_length(base_key_length: u32) -> u32 {
    return (base_key_length / 2) - 1;
}

fn upper_key_lpos(l: u32) -> u32 {
    return l + 1;
}

fn lower_key_lpos(l: u32, base_key_length: u32) -> u32 {
    return l + 1 + upper_key_length(base_key_length);
}

fn draw_left_white_key_vert(canvas: &mut SimpleBuffer, x: u32, y: u32, color: Color, key_thickness: u32, base_key_length: u32) {
    drawing::blend_rect(
        canvas, 
        x - ((key_thickness - 2) / 2), 
        upper_key_lpos(y),
        key_thickness - 1, 
        full_key_length(base_key_length),
        color);
    drawing::blend_rect(canvas, 
        x + ((key_thickness + 1) / 2), 
        lower_key_lpos(y, base_key_length), 
        key_thickness / 2, 
        lower_key_length(base_key_length), 
        color);
}

fn draw_center_white_key_vert(canvas: &mut SimpleBuffer, x: u32, y: u32, color: Color, key_thickness: u32, base_key_length: u32) {
    drawing::blend_rect(canvas, 
        x - ((key_thickness - 2) / 2), 
        upper_key_lpos(y),
        key_thickness - 1, 
        upper_key_length(base_key_length), 
        color);
    drawing::blend_rect(canvas, 
        x - (key_thickness - 1), 
        lower_key_lpos(y, base_key_length),
        (key_thickness * 2) - 1, 
        lower_key_length(base_key_length),
        color);
}

fn draw_right_white_key_vert(canvas: &mut SimpleBuffer, x: u32, y: u32, color: Color, key_thickness: u32, base_key_length: u32) {
    drawing::blend_rect(canvas, 
        x - ((key_thickness - 2) / 2), 
        upper_key_lpos(y),
        key_thickness - 1, 
        full_key_length(base_key_length),
        color);
    drawing::blend_rect(canvas, 
        x - (key_thickness - 1),
        lower_key_lpos(y, base_key_length),
        (key_thickness + 1) / 2, 
        lower_key_length(base_key_length), 
        color);
}

fn draw_topmost_white_key_vert(canvas: &mut SimpleBuffer, x: u32, y: u32, color: Color, key_thickness: u32, base_key_length: u32) {
    drawing::blend_rect(canvas, 
        x - ((key_thickness - 2) / 2), 
        upper_key_lpos(y),
        key_thickness + ((key_thickness - 2) / 2), 
        full_key_length(base_key_length),
        color);
}

fn draw_black_key_vert(canvas: &mut SimpleBuffer, x: u32, y: u32, color: Color, key_thickness: u32, base_key_length: u32) {
    drawing::blend_rect(canvas, 
        x - (key_thickness / 2),
        upper_key_lpos(y),
        key_thickness + 1,
        upper_key_length(base_key_length),
        color);
}

fn draw_speaker_key_vert(canvas: &mut SimpleBuffer, color: Color, x: u32, y: u32, key_thickness: u32, base_key_length: u32) {
    drawing::blend_rect(canvas, 
        x - (key_thickness / 2),
        upper_key_lpos(y),
        key_thickness + 1,
        full_key_length(base_key_length),
        color);
}

fn collect_channels<'a>(apu: &'a ApuState, mapper: &'a dyn Mapper) -> Vec<&'a dyn AudioChannelState> {
    let mut channels: Vec<& dyn AudioChannelState> = Vec::new();
    channels.extend(apu.channels());
    channels.extend(mapper.channels());
    channels.push(apu);
    return channels;
}

fn midi_frequency(midi_index: u32) -> f32 {
    return 440.0 * (2.0_f32).powf(((midi_index as f32) - 69.0) / 12.0);
}

fn midi_index(note_name: &str) -> Result<u32, String> {
     let re = Regex::new(r"([A-Ga-g])([BbSs#]?)(\d+)").unwrap();
     if re.is_match(note_name) {
        let captures = re.captures(note_name).unwrap();

        let letter_name = captures[1].to_string().to_ascii_lowercase();
        let letter_index = match letter_name.as_str() {
            "c" => 0,
            "d" => 2,
            "e" => 4,
            "f" => 5,
            "g" => 7,
            "a" => 9,
            "b" => 11,
            _ => 0 // should be unreachable
        };

        let modifier: i32 = match &captures[2] {
            "B" => -1,
            "b" => -1,
            "S" => 1,
            "s" => 1,
            "#" => 1,
            _ => 0
        };

        let octave_number: i32 = captures[3].parse().expect("Invalid octave number");
        let octave_index = octave_number * 12;

        let note_index = octave_index + letter_index + modifier;
        if note_index >= 0 {
            return Ok((note_index) as u32);
        } else {
            return Err(format!("Invalid MIDI index: {}", note_index));
        }
     } else {
        return Err(format!("Invalid MIDI name: {}", note_name));
     }
}

pub fn default_channel_colors() -> HashMap<String, HashMap<String, Vec<Color>>> {
    let mut channel_colors: HashMap<String, HashMap<String, Vec<Color>>> = HashMap::new();

    let mut apu_colors: HashMap<String, Vec<Color>> = HashMap::new();
    apu_colors.insert("Pulse 1".to_string(), vec!(
        Color::rgb(0xFF, 0xA0, 0xA0),   // 12.5
        Color::rgb(0xFF, 0x40, 0xFF),   // 25
        Color::rgb(0xFF, 0x40, 0x40),   // 50
        Color::rgb(0xFF, 0x40, 0xFF))); // 75 (same as 25)
    apu_colors.insert("Pulse 2".to_string(), vec!(
        Color::rgb(0xFF, 0xE0, 0xA0),   // 12.5
        Color::rgb(0xFF, 0xC0, 0x40),   // 25
        Color::rgb(0xFF, 0xFF, 0x40),   // 50
        Color::rgb(0xFF, 0xC0, 0x40))); // 75 (same as 25)
    apu_colors.insert("Triangle".to_string(), vec!(Color::rgb(0x40, 0xFF, 0x40)));
    apu_colors.insert("Noise".to_string(), vec!(
        Color::rgb(192, 192, 192),
        Color::rgb(128, 240, 255)));
    apu_colors.insert("DMC".to_string(), vec!(Color::rgb(96,  32, 192)));

    let mut vrc6_colors: HashMap<String, Vec<Color>> = HashMap::new();
    vrc6_colors.insert("Pulse 1".to_string(), vec!(
        Color::rgb(0xf2, 0xbb, 0xd8),   // 6.25%
        Color::rgb(0xdb, 0xa0, 0xbf),   // 12.5%
        Color::rgb(0xc4, 0x86, 0xa6),   // 18.75%
        Color::rgb(0xad, 0x6c, 0x8d),   // 25%
        Color::rgb(0x97, 0x51, 0x74),   // 31.25%
        Color::rgb(0x80, 0x37, 0x5b),   // 37.5%
        Color::rgb(0x69, 0x1d, 0x42),   // 43.75%
        Color::rgb(0x53, 0x03, 0x2a))); // 50%
    vrc6_colors.insert("Pulse 2".to_string(), vec!(
        Color::rgb(0xe8, 0xa7, 0xe7),   // 6.25%
        Color::rgb(0xd2, 0x8f, 0xd1),   // 12.5%
        Color::rgb(0xbd, 0x78, 0xbb),   // 18.75%
        Color::rgb(0xa7, 0x60, 0xa6),   // 25%
        Color::rgb(0x92, 0x49, 0x90),   // 31.25%
        Color::rgb(0x7c, 0x31, 0x7b),   // 37.5%
        Color::rgb(0x67, 0x1a, 0x65),   // 43.75%
        Color::rgb(0x52, 0x03, 0x50))); // 50%
    vrc6_colors.insert("Sawtooth".to_string(), vec!(
        Color::rgb(0x07, 0x7d, 0x5a),   // Normal
        Color::rgb(0x9f, 0xb8, 0xed))); // Distortion

    let mut mmc5_colors: HashMap<String, Vec<Color>> = HashMap::new();
    mmc5_colors.insert("Pulse 1".to_string(), vec!(Color::rgb(224, 24, 64)));
    mmc5_colors.insert("Pulse 2".to_string(), vec!(Color::rgb(180, 12, 40)));
    mmc5_colors.insert("PCM".to_string(), vec!(Color::rgb(224, 24, 64)));

    let mut s5b_colors: HashMap<String, Vec<Color>> = HashMap::new();
    s5b_colors.insert("A".to_string(), vec!(Color::rgb(32, 144, 204)));
    s5b_colors.insert("B".to_string(), vec!(Color::rgb(24, 104, 228)));
    s5b_colors.insert("C".to_string(), vec!(Color::rgb(16, 64, 248)));

    let mut n163_colors: HashMap<String, Vec<Color>> = HashMap::new();
    // TODO: Fix these. Even for defaults they're too dark and ugly.
    let wavetable_gradient = vec!(
        Color::rgb(0x66, 0x0e, 0x0e),
        Color::rgb(0xc9, 0x9c, 0x9c),
    );
    n163_colors.insert("NAMCO 1".to_string(), wavetable_gradient.clone());
    n163_colors.insert("NAMCO 2".to_string(), wavetable_gradient.clone());
    n163_colors.insert("NAMCO 3".to_string(), wavetable_gradient.clone());
    n163_colors.insert("NAMCO 4".to_string(), wavetable_gradient.clone());
    n163_colors.insert("NAMCO 5".to_string(), wavetable_gradient.clone());
    n163_colors.insert("NAMCO 6".to_string(), wavetable_gradient.clone());
    n163_colors.insert("NAMCO 7".to_string(), wavetable_gradient.clone());
    n163_colors.insert("NAMCO 8".to_string(), wavetable_gradient.clone());

    channel_colors.insert("2A03".to_string(), apu_colors);
    channel_colors.insert("VRC6".to_string(), vrc6_colors);
    channel_colors.insert("MMC5".to_string(), mmc5_colors);
    channel_colors.insert("YM2149F".to_string(), s5b_colors);
    channel_colors.insert("N163".to_string(), n163_colors);

    return channel_colors;
}

pub struct PianoRollWindow {
    pub canvas: SimpleBuffer,
    pub shown: bool,
    pub scale: u32,
    pub keys: u32,
    pub lowest_frequency: f32,
    pub lowest_index: u32,
    pub highest_frequency: f32,
    pub highest_index: u32,
    pub time_slices: VecDeque<Vec<ChannelSlice>>,
    pub polling_counter: usize,

    // user-configurable options
    pub key_thickness: u32,
    pub key_length: u32,
    pub surfboard_height: u32,
    pub scroll_direction: ScrollDirection,
    pub polling_type: PollingType,
    pub speed_multiplier: u32,

    // Keyed on: chip name, then channel name within that chip
    pub channel_colors: HashMap<String, HashMap<String, Vec<Color>>>,
}

impl PianoRollWindow {
    pub fn new() -> PianoRollWindow {
        println!("TESTING");
        println!("Midi name C0 produces: index {}, freq {}", midi_index("C0").unwrap(), midi_frequency(midi_index("C0").unwrap()));
        println!("Midi name Cs8 produces: index {}, freq {}", midi_index("Cs9").unwrap(), midi_frequency(midi_index("Cs9").unwrap()));


        return PianoRollWindow {
            //canvas: SimpleBuffer::new(480, 270), // conveniently 1/4 of 1080p, for easy nearest-neighbor upscaling of captures
            //canvas: SimpleBuffer::new(960, 540), // conveniently 1/2 of 1080p, for easy nearest-neighbor upscaling of captures
            canvas: SimpleBuffer::new(1920, 1080), // actually 1080p
            shown: false,
            scale: 1,
            keys: 109,
            key_thickness: 16,
            key_length: 64,
            surfboard_height: 128,
            lowest_frequency: midi_frequency(midi_index("C0").unwrap()), // ~C0
            lowest_index: midi_index("C0").unwrap(),
            highest_frequency: midi_frequency(midi_index("Cs9").unwrap()), // ~C#8
            highest_index: midi_index("Cs9").unwrap(),
            time_slices: VecDeque::new(),
            polling_counter: 1,
            scroll_direction: ScrollDirection::TopToBottom,
            polling_type: PollingType::ApuQuarterFrame,
            speed_multiplier: 6,
            channel_colors: default_channel_colors(),
        };
    }

    fn roll_width(&self) -> u32 {
        return self.canvas.height - self.key_length - self.surfboard_height;
    }

    fn draw_piano_strings_horiz(&mut self, x: u32, starting_y: u32, width: u32) {
        let white_string = Color::rgb(0x0C, 0x0C, 0x0C);
        let black_string = Color::rgb(0x06, 0x06, 0x06);

        let string_colors = [
            white_string, //C
            black_string, //Db
            white_string, //D
            black_string, //Eb
            white_string, //E
            white_string, //F
            black_string, //Gb
            white_string, //G
            black_string, //Ab
            white_string, //A
            black_string, //Bb
            white_string, //B
        ];

        let mut key_counter = 0;
        let mut y = starting_y;
        let safety_margin = 0 + self.key_thickness * 2;
        while key_counter < self.keys && y > safety_margin {
            let string_color = string_colors[(key_counter % 12) as usize];
            drawing::rect(&mut self.canvas, x, y, width, 1, string_color);
            y -= self.key_thickness;
            key_counter += 1;
        }
    }

    fn draw_piano_strings_vert(&mut self, starting_x: u32, y: u32, height: u32) {
        let white_string = Color::rgb(0x0C, 0x0C, 0x0C);
        let black_string = Color::rgb(0x06, 0x06, 0x06);

        let string_colors = [
            white_string, //C
            black_string, //Db
            white_string, //D
            black_string, //Eb
            white_string, //E
            white_string, //F
            black_string, //Gb
            white_string, //G
            black_string, //Ab
            white_string, //A
            black_string, //Bb
            white_string, //B
        ];

        let mut key_counter = 0;
        let mut x = starting_x;
        let safety_margin = self.canvas.width - self.key_thickness * 2;
        while key_counter < self.keys && x < safety_margin {
            let string_color = string_colors[(key_counter % 12) as usize];
            drawing::rect(&mut self.canvas, x, y, 1, height, string_color);
            x += self.key_thickness; // TODO: it's not "height" anymore, more like key_size?
            key_counter += 1;
        }
    }

    fn draw_waveform_string_horiz(&mut self, x: u32, y: u32, width: u32) {
        let waveform_string = Color::rgb(0x06, 0x06, 0x06);
        // Draw one extra string for the waveform display
        drawing::rect(&mut self.canvas, x, y, width, 1, waveform_string);
    }

    fn draw_waveform_string_vert(&mut self, x: u32, y: u32, height: u32) {
        let waveform_string = Color::rgb(0x06, 0x06, 0x06);
        // Draw one extra string for the waveform display
        drawing::rect(&mut self.canvas, x, y, 1, height, waveform_string);
    }

    // TOTO: this is hard-coded and isn't especially flexible. Shouldn't we use the key spot routines
    // instead of this?
    fn draw_piano_keys_horiz(&mut self, x: u32, base_y: u32) {
        let white_key_border = Color::rgb(0x1C, 0x1C, 0x1C);
        let white_key = Color::rgb(0x20, 0x20, 0x20);
        let black_key = Color::rgb(0x00, 0x00, 0x00);
        let top_edge = Color::rgb(0x0A, 0x0A, 0x0A);

        let upper_key_pixels = [
          white_key, // C
          black_key, black_key, black_key, // Db
          white_key, // D
          black_key, black_key, black_key, // Eb
          white_key, // E
          white_key_border,
          white_key, // F
          black_key, black_key, black_key, // Gb
          white_key, // G
          black_key, black_key, black_key, // Ab
          white_key, // A
          black_key, black_key, black_key, // Bb
          white_key, // B
          white_key_border, 
        ];

        let lower_key_pixels = [
          white_key, // C (bottom half)
          white_key, // C (top half)
          white_key_border,
          white_key, white_key, white_key, // D
          white_key_border, 
          white_key, white_key, // E
          white_key_border,
          white_key, white_key, // F
          white_key_border,
          white_key, white_key, white_key, // G
          white_key_border, 
          white_key, white_key, white_key, // A
          white_key_border, 
          white_key, white_key, // B
          white_key_border,
        ];

        let canvas_height = self.canvas.height;
        drawing::rect(&mut self.canvas, x, 0, 16, canvas_height, top_edge);
        for y in 0 .. self.keys * self.key_thickness - 1 {
            let pixel_index = y % upper_key_pixels.len() as u32;
            drawing::rect(&mut self.canvas, x+0, base_y - y, 8, 1, upper_key_pixels[pixel_index as usize]);
            drawing::rect(&mut self.canvas, x+8, base_y - y, 8, 1, lower_key_pixels[pixel_index as usize]);
        }
        drawing::rect(&mut self.canvas, x, 0, 1, canvas_height, top_edge);
    }

    // TOTO: this is hard-coded and isn't especially flexible. Shouldn't we use the key spot routines
    // instead of this?
    fn draw_piano_keys_vert(&mut self, base_x: u32, y: u32) {
        let white_key_border = Color::rgb(0x1C, 0x1C, 0x1C);
        let white_key = Color::rgb(0x20, 0x20, 0x20);
        let black_key = Color::rgb(0x00, 0x00, 0x00);
        let top_edge = Color::rgb(0x0A, 0x0A, 0x0A);

        let key_colors = [
          white_key, // C
          black_key, // Db
          white_key, // D
          black_key, // Eb
          white_key, // E
          white_key, // F
          black_key, // Gb
          white_key, // G
          black_key, // Ab
          white_key, // A
          black_key, // Bb
          white_key, // B
        ];

        let key_drawing_functions = [
            draw_left_white_key_vert,   //C
            draw_black_key_vert,        //Db
            draw_center_white_key_vert, //D
            draw_black_key_vert,        //Eb
            draw_right_white_key_vert,  //E
            draw_left_white_key_vert,   //F
            draw_black_key_vert,        //Gb
            draw_center_white_key_vert, //G
            draw_black_key_vert,        //Ab
            draw_center_white_key_vert, //A
            draw_black_key_vert,        //Bb
            draw_right_white_key_vert,  //B
        ];

        let canvas_width = self.canvas.width;
        drawing::rect(&mut self.canvas, 0, y, canvas_width, self.key_length, top_edge);
        drawing::rect(&mut self.canvas, base_x, y, self.keys * self.key_thickness, self.key_length, white_key_border);
        for key_index in 0 .. self.keys - 1 {
            let x = base_x + key_index * self.key_thickness;
            key_drawing_functions[key_index as usize % 12](&mut self.canvas, x, y, key_colors[key_index as usize % 12], self.key_thickness, self.key_length);
        }
        let topmost_x = base_x + (self.keys - 1) * self.key_thickness;
        draw_topmost_white_key_vert(&mut self.canvas, topmost_x, y, white_key, self.key_thickness, self.key_length);
        drawing::rect(&mut self.canvas, 0, y, canvas_width, 1, top_edge);
    }

    fn draw_key_spot_horiz(canvas: &mut SimpleBuffer, slice: &ChannelSlice, key_height: u32, x: u32, starting_y: u32) {
        if !slice.visible {return;}

        match slice.note_type {
            NoteType::Waveform => {
                let mut base_color = slice.color;
                let volume_percent = slice.thickness / 6.0;
                base_color.set_alpha((volume_percent * 255.0) as u8);
                draw_speaker_key_horiz(canvas, base_color, x, ((starting_y as f32) - slice.y * (key_height as f32)) as u32);
            },
            _ => {
                let key_drawing_functions = [
                    draw_left_white_key_horiz,   //C
                    draw_black_key_horiz,        //Db
                    draw_center_white_key_horiz, //D
                    draw_black_key_horiz,        //Eb
                    draw_right_white_key_horiz,  //E
                    draw_left_white_key_horiz,   //F
                    draw_black_key_horiz,        //Gb
                    draw_center_white_key_horiz, //G
                    draw_black_key_horiz,        //Ab
                    draw_center_white_key_horiz, //A
                    draw_black_key_horiz,        //Bb
                    draw_right_white_key_horiz,  //B
                ];

                let mut base_color = slice.color;

                let note_key = slice.y;
                let base_key = note_key.floor();
                let adjacent_key = note_key.ceil();

                let base_volume_percent = slice.thickness / 6.0;
                let adjusted_volume_percent = 0.05 + base_volume_percent * 0.95;
                let base_percent = (1.0 - (note_key % 1.0)) * adjusted_volume_percent;
                let adjacent_percent = (note_key % 1.0) * adjusted_volume_percent;

                let base_y = (starting_y as f32) - base_key * key_height as f32;
                if base_y > 1.0 && base_y < (canvas.height - 2) as f32 {
                    base_color.set_alpha((base_percent * 255.0) as u8);
                    key_drawing_functions[base_key as usize % 12](canvas, x, base_y as u32, base_color);
                }

                let adjacent_y = (starting_y as f32) - adjacent_key * key_height as f32;
                if adjacent_y > 1.0 && adjacent_y < (canvas.height - 2) as f32 {
                    base_color.set_alpha((adjacent_percent * 255.0) as u8);
                    key_drawing_functions[adjacent_key as usize % 12](canvas, x, adjacent_y as u32, base_color);
                }
            }
        }        
    }

    fn draw_key_spot_vert(canvas: &mut SimpleBuffer, slice: &ChannelSlice, key_thickness: u32, key_length: u32, starting_x: u32, y: u32) {
        if !slice.visible {return;}

        match slice.note_type {
            NoteType::Waveform => {
                let mut base_color = slice.color;
                let volume_percent = slice.thickness / 6.0;
                base_color.set_alpha((volume_percent * 255.0) as u8);
                //draw_speaker_key_horiz(canvas, base_color, ((starting_x as f32) - slice.y * (key_width as f32)) as u32, y);
            },
            _ => {
                let key_drawing_functions = [
                    draw_left_white_key_vert,   //C
                    draw_black_key_vert,        //Db
                    draw_center_white_key_vert, //D
                    draw_black_key_vert,        //Eb
                    draw_right_white_key_vert,  //E
                    draw_left_white_key_vert,   //F
                    draw_black_key_vert,        //Gb
                    draw_center_white_key_vert, //G
                    draw_black_key_vert,        //Ab
                    draw_center_white_key_vert, //A
                    draw_black_key_vert,        //Bb
                    draw_right_white_key_vert,  //B
                ];

                let mut base_color = slice.color;

                let note_key = slice.y;
                let base_key = note_key.floor();
                let adjacent_key = note_key.ceil();

                let base_volume_percent = slice.thickness / 6.0;
                let adjusted_volume_percent = 0.05 + base_volume_percent * 0.95;
                let base_percent = (1.0 - (note_key % 1.0)) * adjusted_volume_percent;
                let adjacent_percent = (note_key % 1.0) * adjusted_volume_percent;

                let base_x = (starting_x as f32) + base_key * key_thickness as f32;
                if base_x > 1.0 && base_x < (canvas.width - key_thickness) as f32 {
                    base_color.set_alpha((base_percent * 255.0) as u8);
                    key_drawing_functions[base_key as usize % 12](canvas, base_x as u32, y, base_color, key_thickness, key_length);
                }

                let adjacent_x = (starting_x as f32) + adjacent_key * key_thickness as f32;
                if adjacent_x > 1.0 && adjacent_x < (canvas.width - key_thickness) as f32 {
                    base_color.set_alpha((adjacent_percent * 255.0) as u8);
                    key_drawing_functions[adjacent_key as usize % 12](canvas, adjacent_x as u32, y, base_color, key_thickness, key_length);
                }
            }
        }        
    }

    fn frequency_to_coordinate(&self, note_frequency: f32) -> f32 {
        let highest_log = self.highest_frequency.ln();
        let lowest_log = self.lowest_frequency.ln();
        let range = highest_log - lowest_log;
        let note_log = note_frequency.ln();
        let piano_roll_height = (self.keys) as f32;
        let coordinate = (note_log - lowest_log) * piano_roll_height / range;
        return coordinate;
    }

    pub fn channel_colors(&self, channel: &dyn AudioChannelState) -> Vec<Color> {
        if channel.muted() {
            return vec!(Color::rgb(32, 32, 32));
        }

        match self.channel_colors.get(&channel.chip()) {
            Some(chip_colors) => {
                match chip_colors.get(&channel.name()) {
                    Some(color_gradient) => {
                        return color_gradient.clone();
                    },
                    None => {
                        // Known chip, but unknown channel within this chip. Weird!
                        // Default to a different grey
                        return vec!(Color::rgb(192,  192, 192));
                    }
                }
            },
            None => {
                // No color is defined for this whole chip. Is it new? Use a default color.
                return vec!(Color::rgb(224, 224, 224));
            }
        }
    }

    fn channel_color(&self, channel: &dyn AudioChannelState) -> Color {
        let colors = self.channel_colors(channel);
        let mut color = colors[0]; // default to the first color
        match channel.timbre() {
            Some(Timbre::DutyIndex{index, max}) => {
                let weight = index as f32 / (max + 1) as f32;
                color = drawing::apply_gradient(colors, weight);
            },
            Some(Timbre::LsfrMode{index, max}) => {
                let weight = index as f32 / (max + 1) as f32;
                color = drawing::apply_gradient(colors, weight);  
            },
            Some(Timbre::PatchIndex{index, max}) => {
                let weight = index as f32 / (max + 1) as f32;
                color = drawing::apply_gradient(colors, weight);  
            }
            None => {},
        }
        return color;
    }

    fn slice_from_channel(&self, channel: &dyn AudioChannelState) -> ChannelSlice {
        if !channel.playing() {
            return ChannelSlice::none();
        }

        let y: f32;
        let thickness: f32 = channel.amplitude() * 6.0;
        let colors = self.channel_colors(channel);
        let mut color = colors[0]; // default to the first color
        let note_type: NoteType;

        match channel.rate() {
            PlaybackRate::FundamentalFrequency{frequency} => {
                y = self.frequency_to_coordinate(frequency);
                note_type = NoteType::Frequency;
            },
            PlaybackRate::LfsrRate{index, max} => {
                note_type = NoteType::Noise;


                // Arbitrarily map all noise frequencies to 16 "strings" since this is what the
                // base 2A03 uses. Accuracy is much less important here.
                let string_coord = (index as f32 / (max + 1) as f32) * 16.0;
                let key_offset = string_coord as f32;
                y = key_offset;

            },
            PlaybackRate::SampleRate{frequency: _} => {
                y = 0.0;
                note_type = NoteType::Waveform;
            }
        }
        
        match channel.timbre() {
            Some(Timbre::DutyIndex{index, max}) => {
                let weight = index as f32 / (max + 1) as f32;
                color = drawing::apply_gradient(colors, weight);
            },
            Some(Timbre::LsfrMode{index, max}) => {
                let weight = index as f32 / (max + 1) as f32;
                color = drawing::apply_gradient(colors, weight);  
            },
            Some(Timbre::PatchIndex{index, max}) => {
                let weight = index as f32 / (max + 1) as f32;
                color = drawing::apply_gradient(colors, weight);  
            }
            None => {},
        }

        return ChannelSlice{
            visible: true,
            y: y,
            thickness: thickness,
            color: color,
            note_type: note_type
        };
    }

    fn draw_slice_horiz(canvas: &mut SimpleBuffer, slice: &ChannelSlice, x: u32, base_y: u32, key_height: u32) {
        if !slice.visible {return;}
        let effective_y = (base_y as f32) - (slice.y * (key_height as f32)) + 0.5;

        let top_edge = effective_y - (slice.thickness / 2.0);
        let bottom_edge = effective_y + (slice.thickness / 2.0);
        let top_floor = top_edge.floor();
        let bottom_floor = bottom_edge.floor();

        // sanity range check:
        if top_edge < 0.0 || bottom_edge > canvas.height as f32 {
            return;
        }

        let mut blended_color = slice.color;
        if top_floor == bottom_floor {
            // Special case: alpha here will be related to their distance. Draw one
            // blended point and exit
            let alpha = bottom_edge - top_edge;
            blended_color.set_alpha((alpha * 255.0) as u8);
            canvas.blend_pixel(x, top_floor as u32, blended_color);
            return;
        }
        // Alpha blend the edges
        let top_alpha = 1.0 - (top_edge - top_floor);
        blended_color.set_alpha((top_alpha * 255.0) as u8);
        canvas.blend_pixel(x, top_floor as u32, blended_color);

        let bottom_alpha = bottom_edge - bottom_floor;
        blended_color.set_alpha((bottom_alpha * 255.0) as u8);
        canvas.blend_pixel(x, bottom_floor as u32, blended_color);

        // If there is any distance at all between the edges, draw a solid color
        // line between them
        for y in (top_floor as u32) + 1 .. bottom_floor as u32 {
            canvas.put_pixel(x, y, slice.color);
        }
    }

    fn draw_slice_vert(canvas: &mut SimpleBuffer, slice: &ChannelSlice, base_x: u32, y: u32, key_width: u32) {
        if !slice.visible {return;}
        let effective_x = (base_x as f32) + (slice.y * (key_width as f32)) + 0.5;

        let left_edge = effective_x - (slice.thickness * (key_width as f32) / 4.0);
        let right_edge = effective_x + (slice.thickness * (key_width as f32) / 4.0);
        let left_floor = left_edge.floor();
        let right_floor = right_edge.floor();

        // sanity range check:
        if left_edge < 0.0 || right_edge > canvas.width as f32 {
            return;
        }

        let mut blended_color = slice.color;
        if left_floor == right_floor {
            // Special case: alpha here will be related to their distance. Draw one
            // blended point and exit
            let alpha = right_edge - left_edge;
            blended_color.set_alpha((alpha * 255.0) as u8);
            canvas.blend_pixel(left_floor as u32, y, blended_color);
            return;
        }
        // Alpha blend the edges
        let left_alpha = 1.0 - (left_edge - left_floor);
        blended_color.set_alpha((left_alpha * 255.0) as u8);
        canvas.blend_pixel(left_floor as u32, y, blended_color);

        let right_alpha = right_edge - right_floor;
        blended_color.set_alpha((right_alpha * 255.0) as u8);
        canvas.blend_pixel(right_floor as u32, y, blended_color);

        // If there is any distance at all between the edges, draw a solid color
        // line between them
        for x in (left_floor as u32) + 1 .. right_floor as u32 {
            canvas.put_pixel(x, y, slice.color);
        }
    }

    fn draw_slices_horiz(&mut self, starting_x: u32, base_y: u32, step_direction: i32) {
        let mut x = starting_x;
        for channel_slice in self.time_slices.iter() {
            for note in channel_slice.iter() {
                PianoRollWindow::draw_slice_horiz(&mut self.canvas, &note, x, base_y, self.key_thickness);
            }
            // bail if we hit either screen edge:
            if x == 0 || x == (self.canvas.width - 1) {
                return; //bail! don't draw offscreen
            }
            x = (x as i32 + step_direction) as u32;
        }
    }

    fn draw_slices_vert(&mut self, base_x: u32, starting_y: u32, step_direction: i32, waveform_pos: u32) {
        let mut y = starting_y;
        for channel_slice in self.time_slices.iter() {
            for note in channel_slice.iter() {
                if note.note_type == NoteType::Waveform {
                    PianoRollWindow::draw_slice_vert(&mut self.canvas, &note, waveform_pos, y, self.key_thickness);
                } else {
                    PianoRollWindow::draw_slice_vert(&mut self.canvas, &note, base_x, y, self.key_thickness);
                }
            }
            // bail if we hit either screen edge:
            if (y as i32 + step_direction) == 0 || y == (self.canvas.height - 1) {
                return; //bail! don't draw offscreen
            }
            y = (y as i32 + step_direction) as u32;

        }
    }

    fn draw_key_spots_horiz(&mut self, x: u32, base_y: u32) {
        for note in self.time_slices.front().unwrap_or(&Vec::new()) {
            PianoRollWindow::draw_key_spot_horiz(&mut self.canvas, &note, self.key_thickness, x, base_y);
        }
    }

    fn draw_key_spots_vert(&mut self, base_x: u32, y: u32, waveform_pos: u32) {
        for note in self.time_slices.front().unwrap_or(&Vec::new()) {
            if note.note_type == NoteType::Waveform {
                if note.visible {
                    let mut base_color = note.color;
                    let volume_percent = note.thickness / 6.0;
                    base_color.set_alpha((volume_percent * 255.0) as u8);
                    draw_speaker_key_vert(&mut self.canvas, base_color, waveform_pos, y - 1, self.key_thickness, self.key_length); 
                }
            } else {
               PianoRollWindow::draw_key_spot_vert(&mut self.canvas, &note, self.key_thickness, self.key_length, base_x, y);
            }
        }
    }

    fn draw_key_spots_vert_inverted(&mut self, base_x: u32, y: u32, waveform_pos: u32) {
        for note in self.time_slices.back().unwrap_or(&Vec::new()) {
            if note.note_type == NoteType::Waveform {
                if note.visible {
                    let mut base_color = note.color;
                    let volume_percent = note.thickness / 6.0;
                    base_color.set_alpha((volume_percent * 255.0) as u8);
                    draw_speaker_key_vert(&mut self.canvas, base_color, waveform_pos, y - 1, self.key_thickness, self.key_length); 
                }
            } else {
               PianoRollWindow::draw_key_spot_vert(&mut self.canvas, &note, self.key_thickness, self.key_length, base_x, y);
            }
        }
    }

    fn update(&mut self, apu: &ApuState, mapper: &dyn Mapper) {
        let mut channels = apu.channels();
        channels.extend(mapper.channels());

        for _i in 0 .. self.speed_multiplier {
            let mut frame_notes: Vec<ChannelSlice> = Vec::new();
            for channel in &channels {
                frame_notes.push(self.slice_from_channel(*channel));
            }
            self.time_slices.push_front(frame_notes);
        }

        while self.time_slices.len() > self.roll_width() as usize {
            self.time_slices.pop_back();
        }
    }

    pub fn find_edge(edge_buffer: &RingBuffer, window_size: usize) -> usize {
        let start_index = (edge_buffer.index() - window_size) % edge_buffer.buffer().len();
        let mut current_index = start_index;
        for _i in 0 .. (window_size * 4) {
            if edge_buffer.buffer()[current_index] != 0 {
                // center the window on this sample
                return (current_index - (window_size / 2)) % edge_buffer.buffer().len();
            }
            current_index = (current_index - 1) % edge_buffer.buffer().len();
        }
        // couldn't find an edge, so return the most recent slice
        return start_index;
    }

    fn draw_vertical_antialiased_line(&mut self, x: u32, top_edge: f32, bottom_edge: f32, color: Color) {
        let top_floor = top_edge.floor();
        let bottom_floor = bottom_edge.floor();
        let canvas = &mut self.canvas;

        let mut blended_color = color;
        if top_floor == bottom_floor {
            // Special case: alpha here will be related to their distance. Draw one
            // blended point and exit
            let alpha = bottom_edge - top_edge;
            blended_color.set_alpha((alpha * 255.0) as u8);
            canvas.blend_pixel(x, top_floor as u32, blended_color);
            return;
        }
        // Alpha blend the edges
        let top_alpha = 1.0 - (top_edge - top_floor);
        blended_color.set_alpha((top_alpha * 255.0) as u8);
        if top_floor > 0.0 && (top_floor as u32) < canvas.height {
            canvas.blend_pixel(x, top_floor as u32, blended_color);
        }

        let bottom_alpha = bottom_edge - bottom_floor;
        blended_color.set_alpha((bottom_alpha * 255.0) as u8);
        if bottom_floor > 0.0 && (bottom_floor as u32) < canvas.height {
            canvas.blend_pixel(x, bottom_floor as u32, blended_color);
        }

        // If there is any distance at all between the edges, draw a solid color
        // line between them
        for y in (top_floor as u32) + 1 .. bottom_floor as u32 {
            if y > 0 && y < canvas.height {
                canvas.put_pixel(x, y, color);
            }
        }
    }

    fn scale_color(original_color: Color, scale_factor: f32) -> Color {
        return Color::rgb(
            (original_color.r() as f32 * scale_factor) as u8,
            (original_color.g() as f32 * scale_factor) as u8,
            (original_color.b() as f32 * scale_factor) as u8
        );
    }

    fn draw_surfboard_background(&mut self, x: u32, y: u32, width: u32, height: u32, color: Color) {
        let bg_color = PianoRollWindow::scale_color(color, 0.125);
        for row in 0 .. height {
            let weight = 1.0 - ((row as f32 * std::f32::consts::PI) / (height as f32)).sin(); 
            let row_color = PianoRollWindow::scale_color(bg_color, weight);
            drawing::rect(&mut self.canvas, x, y + row, width, 1, row_color);
        }
    }

    fn draw_channel_surfboard(&mut self, channel: &dyn AudioChannelState, x: u32, y: u32, width: u32, height: u32) {
        let color = self.channel_color(channel);
        self.draw_surfboard_background(x, y, width, height, color);

        let speed = 4;
        let first_sample_index = PianoRollWindow::find_edge(channel.edge_buffer(), (width * speed) as usize);
        let sample_min = channel.min_sample();
        let sample_max = channel.max_sample() + 1; // ???
        let range = (sample_max as u32) - (sample_min as u32);
        let sample_buffer = channel.sample_buffer().buffer();
        let mut last_y = ((sample_buffer[first_sample_index] - sample_min) as f32 * height as f32) / range as f32;
        for i in 0 .. width {
            let dx = x + i;
            let sample_index = (first_sample_index + (i * speed) as usize) % sample_buffer.len();
            let sample = sample_buffer[sample_index];
            let current_y = ((sample - sample_min) as f32 * height as f32) / range as f32;
            // Todo: connect last_y to current_y
            // (y'know, not this)
            //self.canvas.put_pixel(dx, y + current_y, color);
            let mut top_edge = current_y;
            let mut bottom_edge = last_y;
            if last_y < current_y {
                top_edge = last_y;
                bottom_edge = current_y;
            }
            let line_thickness = 0.5;
            let glow_thickness = 2.5;
            let glow_color = PianoRollWindow::scale_color(color, 0.25);
            self.draw_vertical_antialiased_line(dx, y as f32 + top_edge - glow_thickness, y as f32 + bottom_edge + glow_thickness, glow_color);
            self.draw_vertical_antialiased_line(dx, y as f32 + top_edge - line_thickness, y as f32 + bottom_edge + line_thickness, color);
            last_y = current_y;
        }
    }

    fn draw_audio_surfboard_horiz(&mut self, runtime: &RuntimeState, x: u32, y: u32, width: u32, height: u32) {
        let channels = collect_channels(&runtime.nes.apu, &*runtime.nes.mapper);
        let channel_width = width / (channels.len() as u32);
        for i in 0 .. channels.len() {
            let channel = channels[i];
            let dx = x + (i as u32) * channel_width;
            self.draw_channel_surfboard(channel, dx, y, channel_width, height);
        }
    }

    pub fn mouse_mutes_channel_horiz(&mut self, runtime: &RuntimeState, sx: u32, sy: u32, width: u32, height: u32, mouse_x: i32, mouse_y: i32) -> Vec<Event> {
        let mut events: Vec<Event> = Vec::new();
        if mouse_x < 0 || mouse_y < 0 {
            return events;
        }
        let mx = mouse_x as u32;
        let my = mouse_y as u32;
        let channels = collect_channels(&runtime.nes.apu, &*runtime.nes.mapper);
        let channel_width = width / (channels.len() as u32);
        for i in 0 .. channels.len() {
            let channel = channels[i];
            let cx = sx + (i as u32) * channel_width;
            if mx >= cx && mx < cx + channel_width && my >= sy && my < sy + height {
               if channel.muted() {
                    events.push(Event::UnmuteChannel(i))
                } else {
                    events.push(Event::MuteChannel(i))
                } 
            }
        }
        return events;
    }

    fn draw_right_to_left(&mut self) {
        let waveform_area_height = 32;
        let waveform_string_pos = self.canvas.height - 16;
        let key_width = 16;
        let bottom_key = self.canvas.height - waveform_area_height;
        let string_width = self.canvas.width - key_width;

        self.draw_piano_strings_horiz(0, bottom_key, string_width);
        self.draw_waveform_string_horiz(0, waveform_string_pos, string_width);
        self.draw_piano_keys_horiz(string_width, bottom_key);
        //draw_speaker_key(&mut self.canvas, black_key);
        self.draw_slices_horiz(string_width, bottom_key, -1);
        self.draw_key_spots_horiz(string_width, bottom_key);
    }

    fn draw_left_to_right(&mut self) {
        let waveform_area_height = 32;
        let waveform_string_pos = self.canvas.height - 16;
        let key_width = 16;
        let bottom_key = self.canvas.height - waveform_area_height;
        let string_width = self.canvas.width - key_width;

        self.draw_piano_strings_horiz(key_width, bottom_key, string_width);
        self.draw_waveform_string_horiz(key_width, waveform_string_pos, string_width);
        self.draw_piano_keys_horiz(0, bottom_key);

        //draw_speaker_key(&mut self.canvas, black_key);

        self.draw_slices_horiz(key_width, bottom_key, 1);
        self.draw_key_spots_horiz(0, bottom_key);
    }

    fn draw_top_to_bottom(&mut self, runtime: &RuntimeState) {
        let waveform_area_width = self.key_thickness * 4;
        let waveform_string_pos = self.key_thickness * 2;
        let waveform_margin = self.key_thickness / 2;
        let key_height = self.key_length;
        let leftmost_key = waveform_area_width + waveform_margin;
        let surfboard_height = self.surfboard_height;
        let string_height = self.canvas.height - key_height - surfboard_height;

        self.draw_piano_strings_vert(waveform_area_width + waveform_margin, surfboard_height + key_height, string_height);
        self.draw_waveform_string_vert(waveform_string_pos, surfboard_height + key_height, string_height);
        self.draw_piano_keys_vert(leftmost_key, surfboard_height);

        self.draw_slices_vert(waveform_area_width + waveform_margin, surfboard_height + key_height, 1, waveform_string_pos);
        self.draw_key_spots_vert(leftmost_key, surfboard_height, waveform_string_pos);
        
        self.draw_audio_surfboard_horiz(runtime, 0, 0, self.canvas.width, surfboard_height);
    }

    fn draw_bottom_to_top(&mut self) {
        let waveform_area_width = 32;
        let waveform_string_pos = 16;
        let key_height = 16;
        let leftmost_key = waveform_area_width;
        let string_height = self.canvas.height - key_height;

        self.draw_piano_strings_vert(waveform_area_width, 0, string_height);
        self.draw_waveform_string_vert(waveform_string_pos, 0, string_height);
        self.draw_piano_keys_vert(leftmost_key, self.canvas.height - key_height);

        self.draw_slices_vert(waveform_area_width, self.canvas.height - key_height, -1, waveform_string_pos);
        self.draw_key_spots_vert(leftmost_key, self.canvas.height - key_height, waveform_string_pos);
    }

    fn draw_player_piano(&mut self) {
        let waveform_area_width = 32;
        let waveform_string_pos = 16;
        let key_height = 16;
        let leftmost_key = waveform_area_width;
        let string_height = self.canvas.height - key_height;

        self.draw_piano_strings_vert(waveform_area_width, 0, string_height);
        self.draw_waveform_string_vert(waveform_string_pos, 0, string_height);
        self.draw_piano_keys_vert(leftmost_key, self.canvas.height - key_height);

        self.draw_slices_vert(waveform_area_width, 1, 1, waveform_string_pos);
        self.draw_key_spots_vert_inverted(leftmost_key, self.canvas.height - key_height, waveform_string_pos);
    }

    fn draw(&mut self, runtime: &RuntimeState) {
        let width = self.canvas.width;
        let height = self.canvas.height;
        drawing::rect(&mut self.canvas, 0, 0, width, height, Color::rgb(0,0,0));
        match self.scroll_direction {
            ScrollDirection::RightToLeft => {self.draw_right_to_left()},
            ScrollDirection::LeftToRight => {self.draw_left_to_right()},
            ScrollDirection::TopToBottom => {self.draw_top_to_bottom(runtime)},
            ScrollDirection::BottomToTop => {self.draw_bottom_to_top()},
            ScrollDirection::PlayerPiano => {self.draw_player_piano()}
        }
    }

    fn mouse_click(&mut self, runtime: &RuntimeState, mx: i32, my: i32) -> Vec<Event> {
        match self.scroll_direction {
            ScrollDirection::TopToBottom => {
                return self.mouse_mutes_channel_horiz(runtime, 0, 0, self.canvas.width, self.surfboard_height, mx, my);
            },
            _ => {
                /* unimplemented */
                return Vec::new();
            }
        }
    }

    fn set_canvas_height(&mut self, height: u32, width: u32) {
        self.canvas = SimpleBuffer::new(height, width);
    }

    fn set_starting_octave(&mut self, octave_number: u32) {
        let note_name = format!("C{}", octave_number);

        let key_index = midi_index(&note_name).unwrap();
        let key_freq = midi_frequency(key_index);
        let highest_index = key_index + self.keys;
        let highest_freq = midi_frequency(highest_index);

        println!("New highest index: {}, new highest freq: {}", highest_index, highest_freq);

        self.lowest_index = key_index;
        self.lowest_frequency = key_freq;
        self.highest_index = highest_index;
        self.highest_frequency = highest_freq;
    }

    fn set_octave_count(&mut self, octave_count: u32) {
        let key_count = octave_count * 12 + 1;

        let highest_index = self.lowest_index + key_count;
        let highest_freq = midi_frequency(highest_index);

        println!("New highest index: {}, new highest freq: {}", highest_index, highest_freq);

        self.keys = key_count;
        self.highest_index = highest_index;
        self.highest_frequency = highest_freq;
    }

    fn apply_color_string(&mut self, chip_name: &str, channel_name: &str, setting_name: &str, color_string: String) {
        let setting_to_index_mapping = HashMap::from([
            // Triangle, DMC, a few other simple chips
            ("static", 0),
            // 2A03, MMC5 and VRC6 pulses
            ("duty0", 0),
            ("duty1", 1),
            ("duty2", 2),
            ("duty3", 3),
            ("duty4", 3),
            ("duty5", 3),
            ("duty6", 3),
            ("duty7", 3),
            // Noise
            ("mode0", 0),
            ("mode1", 1),
            // Two-color gradients (N163)
            ("gradient_low", 0),
            ("gradient_high", 1),
        ]);

        match self.channel_colors.get_mut(chip_name) {
            Some(chip_colors) => {
                match chip_colors.get_mut(channel_name) {
                    Some(channel_gradient) => {
                        match setting_to_index_mapping.get(setting_name) {
                            Some(setting_index) => {
                                match Color::from_string(&color_string) {
                                    Ok(color) => {channel_gradient[*setting_index] = color},
                                    Err(_) => {
                                        println!("Warning: Invalid color string {}, ignoring.", color_string);
                                    }
                                }
                            },
                            None => {
                                println!("Warning: setting {} does not correspond to any color slot for channel {} on chip {}", setting_name, channel_name, chip_name);
                            }
                        }
                    },
                    None => {
                        println!("Warning: Failed to apply color string {} to unknown channel {}", color_string, channel_name);
                    }
                }
            },
            None => {
                println!("Warning: Failed to apply color string {} to unknown audio chip {}", color_string, chip_name);
            }
        }
    }
}

impl Panel for PianoRollWindow {
    fn title(&self) -> &str {
        return "Piano Roll";
    }

    fn shown(&self) -> bool {
        return self.shown;
    }

    fn scale_factor(&self) -> u32 {
        return self.scale;
    }

    fn handle_event(&mut self, runtime: &RuntimeState, event: Event) -> Vec<Event> {
        let mut events: Vec<Event> = Vec::new();
        match event {
            Event::NesNewFrame => {
                if self.polling_type == PollingType::PpuFrame {
                    self.update(&runtime.nes.apu, &*runtime.nes.mapper);
                }
            },
            Event::NesNewScanline => {
                if self.polling_type == PollingType::PpuScanline {
                    self.update(&runtime.nes.apu, &*runtime.nes.mapper);
                }
            },
            Event::NesNewApuQuarterFrame => {
                if self.polling_type == PollingType::ApuQuarterFrame {
                    self.update(&runtime.nes.apu, &*runtime.nes.mapper);
                }
            },
            Event::NesNewApuHalfFrame => {
                if self.polling_type == PollingType::ApuHalfFrame {
                    self.update(&runtime.nes.apu, &*runtime.nes.mapper);
                }
            },
            Event::MouseClick(x, y) => {events.extend(self.mouse_click(runtime, x, y));},
            Event::RequestFrame => {self.draw(runtime)},
            Event::ShowPianoRollWindow => {self.shown = true},
            Event::CloseWindow => {self.shown = false},

            Event::ApplyIntegerSetting(path, value) => {
                match path.as_str() {
                    "piano_roll.canvas_width" => {self.set_canvas_height(value as u32, self.canvas.height)},
                    "piano_roll.canvas_height" => {self.set_canvas_height(self.canvas.width, value as u32)},
                    "piano_roll.key_thickness" => {self.key_thickness = value as u32},
                    "piano_roll.key_length" => {self.key_length = value as u32},
                    "piano_roll.octave_count" => {self.set_octave_count(value as u32)},
                    "piano_roll.scale_factor" => {self.scale = value as u32},
                    "piano_roll.speed_multiplier" => {self.speed_multiplier = value as u32},
                    "piano_roll.starting_octave" => {self.set_starting_octave(value as u32)},
                    "piano_roll.waveform_height" => {self.surfboard_height = value as u32},
                    _ => {}
                }
            },

            Event::ApplyStringSetting(path, value) => {
                let components = path.split(".").collect::<Vec<&str>>();
                if components.len() == 5 && components[0] == "piano_roll" && components[1] == "colors" {
                    self.apply_color_string(components[2], components[3], components[4], value);
                }
            }
            _ => {}
        }
        return events;
    }
    
    fn active_canvas(&self) -> &SimpleBuffer {
        return &self.canvas;
    }
}