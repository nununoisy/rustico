use crate::app;

use rustico_ui_common::application::RuntimeState as RusticoRuntimeState;
use rustico_ui_common::events;
use rustico_ui_common::game_window::GameWindow;
use rustico_ui_common::panel::Panel;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use std::sync::mpsc::{Receiver, Sender, TryRecvError};

lazy_static! {
    pub static ref AUDIO_OUTPUT_BUFFER: Mutex<VecDeque<f32>> = Mutex::new(VecDeque::new());
}

pub struct RenderedImage {
    pub width: usize,
    pub height: usize,
    pub scale: usize,
    pub rgba_buffer: Vec<u8>,
}

struct Worker {
    runtime_rx: Receiver<events::Event>,
    shell_tx: Sender<app::ShellEvent>,

    // We need to keep the audio stream around so that it continues to run, but
    // we never need to read it directly. Rust complains about this. :)
    _audio_stream: Box<dyn StreamTrait>,
    runtime_state: RusticoRuntimeState,
    game_window: GameWindow,

    exit_requested: bool,
}

impl Worker {
    pub fn new(runtime_rx: Receiver<events::Event>, shell_tx: Sender<app::ShellEvent>) -> Worker {
        let audio_stream = setup_audio_stream();
        let runtime_state = RusticoRuntimeState::new();
        let game_window = GameWindow::new();

        return Worker{
            runtime_rx: runtime_rx,
            shell_tx: shell_tx,
            _audio_stream: audio_stream,
            runtime_state: runtime_state,
            game_window: game_window,
            exit_requested: false
        };
    }

    pub fn process_incoming_events(&mut self) {
        loop {
            match self.runtime_rx.try_recv() {
                Ok(event) => {
                    self.dispatch_event(event);
                },
                Err(error) => {
                    match error {
                        TryRecvError::Empty => {
                            // all done!
                            return
                        },
                        TryRecvError::Disconnected => {
                            // PANIC AT THE DISCO, ALL HOPE IS LOST!
                            // (We're just shutting down, it's fine)
                            return
                        }
                    }
                }
            }
        }
    }

    pub fn dispatch_event(&mut self, event: events::Event) {
        let mut responses: Vec<events::Event> = Vec::new();
        responses.extend(self.runtime_state.handle_event(event.clone()));
        responses.extend(self.game_window.handle_event(&self.runtime_state, event.clone()));
        responses.extend(self.handle_event(event.clone()));
        for response in responses {
            self.dispatch_event(response);
        }
    }

    pub fn handle_event(&mut self, event: events::Event) -> Vec<events::Event> {
        // For now, the WORKER doesn't need to do anything with runtime events. Later it might
        // and this is where those would get handled. Setting this up now for consistency.
        let events: Vec<events::Event> = Vec::new();
        match event {
            rustico_ui_common::Event::CartridgeLoaded(_id) => {
                let has_sram = self.runtime_state.nes.mapper.has_sram();
                let _ = self.shell_tx.send(app::ShellEvent::HasSram(has_sram));
            }
            rustico_ui_common::Event::SaveSram(sram_id, sram_data) => {
                self.save_sram(sram_id, &sram_data);
            },
            rustico_ui_common::Event::CloseApplication => {
                println!("WORKER: application close requested, will exit after processing remaining events...");
                self.exit_requested = true;
            },
            rustico_ui_common::Event::ApplyBooleanSetting(_,_) => {
                let _ = self.shell_tx.send(app::ShellEvent::SettingsUpdated(
                    Arc::new(self.runtime_state.settings.clone())
                ));
            },
            rustico_ui_common::Event::ApplyIntegerSetting(_,_) => {
                let _ = self.shell_tx.send(app::ShellEvent::SettingsUpdated(
                    Arc::new(self.runtime_state.settings.clone())
                ));
            },
            rustico_ui_common::Event::ApplyFloatSetting(_,_) => {
                let _ = self.shell_tx.send(app::ShellEvent::SettingsUpdated(
                    Arc::new(self.runtime_state.settings.clone())
                ));
            },
            rustico_ui_common::Event::ApplyStringSetting(_,_) => {
                let _ = self.shell_tx.send(app::ShellEvent::SettingsUpdated(
                    Arc::new(self.runtime_state.settings.clone())
                ));
            },
            _ => {}
        }
        return events;
    }

    pub fn save_sram(&self, filename: String, sram_data: &[u8]) {
        let file = File::create(filename.clone());
        match file {
            Err(why) => {
                println!("Couldn't open {}: {}", filename, why.to_string());
            },
            Ok(mut file) => {
                let _ = file.write_all(sram_data);
                println!("Wrote sram data to: {}", filename);
            },
        };
    }

    pub fn step_emulator(&mut self) {
        // Quickly poll the length of the audio buffer
        let audio_output_buffer = AUDIO_OUTPUT_BUFFER.lock().expect("wat");
        let mut output_buffer_len = audio_output_buffer.len();
        drop(audio_output_buffer); // immediately free the mutex, so running the emulator doesn't starve the audio thread

        // Now we do fun stuff: as long as we are under the audio threshold, run one scanline. If we happen
        // to complete a frame while doing this, update the game window texture (and later, call "draw" on all
        // active subwindows so they know to repaint)
        // (2048 is arbitrary, make this configurable later!)
        let mut repaint_needed = false;
        while output_buffer_len < 512 {
            self.dispatch_event(events::Event::NesRunScanline);
            if self.runtime_state.nes.ppu.current_scanline == 242 {
                // we just finished a game frame, so have the game window repaint itself
                self.dispatch_event(events::Event::RequestFrame);
                repaint_needed = true;
            }
            let samples_i16 = self.runtime_state.nes.apu.consume_samples();
            let samples_float: Vec<f32> = samples_i16.into_iter().map(|x| <i16 as Into<f32>>::into(x) / 32767.0).collect();
            // Apply those samples to the audio buffer AND recheck our count
            // (keep going until we rise above the threshold)
            let mut audio_output_buffer = AUDIO_OUTPUT_BUFFER.lock().expect("wat");
            audio_output_buffer.extend(samples_float);
            output_buffer_len = audio_output_buffer.len();
            drop(audio_output_buffer);
        }

        if repaint_needed {
            let repaint_event = app::ShellEvent::ImageRendered(
                "game_window".to_string(),
                Arc::new(RenderedImage{
                    width: self.game_window.canvas.width as usize,
                    height: self.game_window.canvas.height as usize,
                    scale: if self.game_window.ntsc_filter == true {1} else {self.game_window.scale as usize},
                    rgba_buffer: Vec::from(self.game_window.canvas.buffer.clone())
                })
            );
            let _ = self.shell_tx.send(repaint_event);
        }
    }
}

pub fn setup_audio_stream() -> Box<dyn StreamTrait> {
    // Setup the audio callback, which will ultimately be in charge of trying to step emulation
    let host = cpal::default_host();
    let device = host.default_output_device().expect("no output device available");

    // TODO: eventually we want to present the supported configs to the end user, and let
    // them pick
    let default_output_config = device.default_output_config().unwrap();
    println!("default config would be: {:?}", default_output_config);

    let mut stream_config: cpal::StreamConfig = default_output_config.into();
    stream_config.buffer_size = cpal::BufferSize::Fixed(256);
    stream_config.channels = 1;
    println!("stream config will be: {:?}", stream_config);

    let stream = device.build_output_stream(
        &stream_config.into(),
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            let mut audio_output_buffer = AUDIO_OUTPUT_BUFFER.lock().expect("wat");
            if audio_output_buffer.len() > data.len() {
                let output_samples = audio_output_buffer.drain(0..data.len()).collect::<VecDeque<f32>>();
                for i in 0 .. data.len() {
                    data[i] = output_samples[i];
                }
            } else {
                for sample in data.iter_mut() {
                    *sample = cpal::Sample::EQUILIBRIUM;
                }
            }
        },
        move |err| {
            println!("Audio error occurred: {}", err)
        },
        None // None=blocking, Some(Duration)=timeout
    ).unwrap();

    stream.play().unwrap();

    return Box::new(stream);
}

pub fn worker_main(runtime_rx: Receiver<events::Event>, shell_tx: Sender<app::ShellEvent>) {
    // We don't need to DO anything with the stream, but we do need to keep it around
    // or it will stop playing.
    let mut worker = Worker::new(runtime_rx, shell_tx);

    while worker.exit_requested == false {
        worker.process_incoming_events();
        worker.step_emulator();
        thread::sleep(Duration::from_millis(1));
    }

    // one more time, just in case things arrive out of order
    thread::sleep(Duration::from_millis(1));
    worker.process_incoming_events();
    println!("WORKER: finished! proceeding to exit.")
}