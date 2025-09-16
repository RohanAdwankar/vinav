use rdev::{listen, simulate, display_size, Event, EventType, Key, Button, SimulateError, DisplayError, ListenError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use std::collections::HashMap;

/// Configuration for cursor movement
const INITIAL_MOVE_STEP: f64 = 5.0; // initial slow movement
const MAX_MOVE_STEP: f64 = 25.0; // maximum fast movement
const ACCELERATION_RATE: f64 = 500.0; // how quickly to accelerate
const REPEAT_DELAY_MS: u64 = 50; // delay between repeated movements
const MOVE_DELAY_MS: u64 = 20; // delay between events for OS to catch up

/// Custom error type for our application
#[derive(Debug)]
#[allow(dead_code)]
enum VimNavError {
    Display(DisplayError),
    Listen(ListenError),
    Simulate(SimulateError),
}

impl std::fmt::Display for VimNavError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            VimNavError::Display(e) => write!(f, "Display error: {:?}", e),
            VimNavError::Listen(e) => write!(f, "Listen error: {:?}", e),
            VimNavError::Simulate(e) => write!(f, "Simulate error: {:?}", e),
        }
    }
}

impl std::error::Error for VimNavError {}

impl From<DisplayError> for VimNavError {
    fn from(err: DisplayError) -> Self {
        VimNavError::Display(err)
    }
}

impl From<ListenError> for VimNavError {
    fn from(err: ListenError) -> Self {
        VimNavError::Listen(err)
    }
}

impl From<SimulateError> for VimNavError {
    fn from(err: SimulateError) -> Self {
        VimNavError::Simulate(err)
    }
}

/// Shared state for current cursor position and acceleration
#[derive(Clone)]
struct CursorState {
    x: f64,
    y: f64,
    screen_width: f64,
    screen_height: f64,
    // Acceleration tracking
    pressed_keys: HashMap<Key, Instant>,
    current_speeds: HashMap<Key, f64>,
}

impl CursorState {
    fn new() -> Result<Self, VimNavError> {
        let (w, h) = display_size()?;
        Ok(CursorState {
            x: w as f64 / 2.0, // start in center
            y: h as f64 / 2.0,
            screen_width: w as f64,
            screen_height: h as f64,
            pressed_keys: HashMap::new(),
            current_speeds: HashMap::new(),
        })
    }

    fn start_key_press(&mut self, key: Key) {
        self.pressed_keys.insert(key, Instant::now());
        self.current_speeds.insert(key, INITIAL_MOVE_STEP);
    }

    fn stop_key_press(&mut self, key: Key) {
        self.pressed_keys.remove(&key);
        self.current_speeds.remove(&key);
    }

    fn update_speed(&mut self, key: Key) -> f64 {
        if let Some(start_time) = self.pressed_keys.get(&key) {
            let hold_duration = start_time.elapsed().as_secs_f64();
            let new_speed = (INITIAL_MOVE_STEP + hold_duration * ACCELERATION_RATE).min(MAX_MOVE_STEP);
            self.current_speeds.insert(key, new_speed);
            new_speed
        } else {
            INITIAL_MOVE_STEP
        }
    }

    fn move_left(&mut self) {
        let speed = self.update_speed(Key::KeyH);
        self.x = (self.x - speed).max(0.0);
    }

    fn move_right(&mut self) {
        let speed = self.update_speed(Key::KeyL);
        self.x = (self.x + speed).min(self.screen_width - 1.0);
    }

    fn move_up(&mut self) {
        let speed = self.update_speed(Key::KeyK);
        self.y = (self.y - speed).max(0.0);
    }

    fn move_down(&mut self) {
        let speed = self.update_speed(Key::KeyJ);
        self.y = (self.y + speed).min(self.screen_height - 1.0);
    }

    fn is_key_pressed(&self, key: Key) -> bool {
        self.pressed_keys.contains_key(&key)
    }
}

fn send_event(event_type: &EventType) -> Result<(), SimulateError> {
    let delay = Duration::from_millis(MOVE_DELAY_MS);
    match simulate(event_type) {
        Ok(()) => {
            // Let the OS catch up (especially important on macOS)
            thread::sleep(delay);
            Ok(())
        },
        Err(e) => {
            eprintln!("Failed to send event {:?}: {:?}", event_type, e);
            Err(e)
        }
    }
}

fn move_cursor(cursor_state: &Arc<Mutex<CursorState>>) -> Result<(), SimulateError> {
    let state = cursor_state.lock().unwrap();
    send_event(&EventType::MouseMove { x: state.x, y: state.y })
}

fn click_mouse() -> Result<(), SimulateError> {
    // Perform a left mouse click (press and release)
    send_event(&EventType::ButtonPress(Button::Left))?;
    send_event(&EventType::ButtonRelease(Button::Left))?;
    println!("Mouse clicked!");
    Ok(())
}

fn main() -> Result<(), VimNavError> {
    println!("Vim-style navigation with smooth acceleration started!");
    println!("Controls:");
    println!("  h - move cursor left");
    println!("  j - move cursor down");
    println!("  k - move cursor up");
    println!("  l - move cursor right");
    println!("  Enter - left mouse click");
    println!("  Escape - quit");
    println!();
    println!("Hold keys longer for faster movement!");

    // Initialize cursor state
    let cursor_state = Arc::new(Mutex::new(CursorState::new()?));
    
    // Move cursor to initial position
    move_cursor(&cursor_state)?;
    println!("Cursor initialized at center of screen");

    // Create a flag to control the movement thread
    let running = Arc::new(Mutex::new(true));
    
    // Start continuous movement thread
    let cursor_state_movement = Arc::clone(&cursor_state);
    let running_movement = Arc::clone(&running);
    
    thread::spawn(move || {
        while *running_movement.lock().unwrap() {
            {
                let mut state = cursor_state_movement.lock().unwrap();
                let mut moved = false;
                
                if state.is_key_pressed(Key::KeyH) {
                    state.move_left();
                    moved = true;
                }
                if state.is_key_pressed(Key::KeyJ) {
                    state.move_down();
                    moved = true;
                }
                if state.is_key_pressed(Key::KeyK) {
                    state.move_up();
                    moved = true;
                }
                if state.is_key_pressed(Key::KeyL) {
                    state.move_right();
                    moved = true;
                }
                
                if moved {
                    drop(state); // Release the lock before calling move_cursor
                    if let Err(e) = move_cursor(&cursor_state_movement) {
                        eprintln!("Failed to move cursor: {:?}", e);
                    }
                }
            }
            
            thread::sleep(Duration::from_millis(REPEAT_DELAY_MS));
        }
    });

    // Set up the event listener
    let cursor_state_clone = Arc::clone(&cursor_state);
    let running_clone = Arc::clone(&running);
    
    let callback = move |event: Event| {
        match event.event_type {
            EventType::KeyPress(key) => {
                match key {
                    Key::KeyH | Key::KeyJ | Key::KeyK | Key::KeyL => {
                        cursor_state_clone.lock().unwrap().start_key_press(key);
                    },
                    Key::Return => {
                        if let Err(e) = click_mouse() {
                            eprintln!("Failed to click mouse: {:?}", e);
                        }
                    },
                    Key::Escape => {
                        println!("Escape pressed, exiting...");
                        *running_clone.lock().unwrap() = false;
                        std::process::exit(0);
                    },
                    _ => {
                        // Ignore other keys
                    }
                }
            },
            EventType::KeyRelease(key) => {
                match key {
                    Key::KeyH | Key::KeyJ | Key::KeyK | Key::KeyL => {
                        cursor_state_clone.lock().unwrap().stop_key_press(key);
                    },
                    _ => {
                        // Ignore other key releases
                    }
                }
            },
            _ => {
                // Ignore other event types
            }
        }
    };

    // Start listening for events (this will block)
    match listen(callback) {
        Ok(()) => {},
        Err(error) => {
            eprintln!("Error listening for events: {:?}", error);
            eprintln!("Note: On macOS, make sure the terminal has Accessibility permissions:");
            eprintln!("System Preferences > Security & Privacy > Privacy > Accessibility");
            *running.lock().unwrap() = false;
            return Err(VimNavError::Listen(error));
        }
    }

    Ok(())
}