use rdev::{simulate, display_size, Event, EventType, Key, Button, SimulateError, DisplayError, GrabError, grab};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use std::collections::HashMap;
use config::{Config, ConfigError, File};
use serde::{Deserialize, Serialize};

/// Configuration structure for vim navigation
#[derive(Debug, Serialize, Deserialize, Clone)]
struct VimNavConfig {
    /// Initial movement speed in pixels
    pub initial_move_step: f64,
    /// Maximum movement speed in pixels (None = unlimited)
    pub max_move_step: Option<f64>,
    /// Exponential base for acceleration (higher = faster acceleration)
    pub acceleration_base: f64,
    /// Multiplier for exponential growth
    pub acceleration_multiplier: f64,
    /// Update rate for movement in milliseconds
    pub repeat_delay_ms: u64,
    /// Delay between OS events in milliseconds
    pub move_delay_ms: u64,
    /// Navigation keys
    pub key_left: String,
    pub key_down: String,
    pub key_up: String,
    pub key_right: String,
    pub key_click: String,
    pub key_insert_mode: String,
    pub key_nav_mode: String,
}

impl Default for VimNavConfig {
    fn default() -> Self {
        Self {
            initial_move_step: 1.0,  // Half the previous speed
            max_move_step: None, // No speed limit by default!
            acceleration_base: 2.0,
            acceleration_multiplier: 50.0,  // Double the multiplier for faster acceleration
            repeat_delay_ms: 30,
            move_delay_ms: 15,
            key_left: "h".to_string(),
            key_down: "j".to_string(),
            key_up: "k".to_string(),
            key_right: "l".to_string(),
            key_click: "return".to_string(),
            key_insert_mode: "i".to_string(),
            key_nav_mode: "escape".to_string(),
        }
    }
}

impl VimNavConfig {
    fn load() -> Result<Self, ConfigError> {
        let settings = Config::builder()
            .add_source(config::Environment::with_prefix("VIMNAV"))
            .add_source(File::with_name("vim_navigation_config").required(false));
        
        // Try to load from config file, fall back to defaults
        match settings.build() {
            Ok(config) => {
                match config.try_deserialize() {
                    Ok(config) => Ok(config),
                    Err(_) => {
                        println!("Using default configuration (config file not found or invalid)");
                        Ok(Self::default())
                    }
                }
            },
            Err(_) => {
                println!("Using default configuration");
                Ok(Self::default())
            }
        }
    }

    fn print_config(&self) {
        println!("=== Current Configuration ===");
        println!("Initial speed: {:.1} px", self.initial_move_step);
        match self.max_move_step {
            Some(max) => println!("Max speed: {:.1} px", max),
            None => println!("Max speed: UNLIMITED"),
        }
        println!("Acceleration base: {:.1}", self.acceleration_base);
        println!("Acceleration multiplier: {:.1}", self.acceleration_multiplier);
        println!("Update rate: {} ms", self.repeat_delay_ms);
        println!("Move delay: {} ms", self.move_delay_ms);
        println!("Navigation keys: {} {} {} {} (left/down/up/right)", 
            self.key_left, self.key_down, self.key_up, self.key_right);
        println!("Control keys: {} (insert), {} (nav mode), {} (click)", 
            self.key_insert_mode, self.key_nav_mode, self.key_click);
        println!();
    }

    fn string_to_key(&self, key_str: &str) -> Option<Key> {
        match key_str.to_lowercase().as_str() {
            "h" => Some(Key::KeyH),
            "j" => Some(Key::KeyJ),
            "k" => Some(Key::KeyK),
            "l" => Some(Key::KeyL),
            "i" => Some(Key::KeyI),
            "return" | "enter" => Some(Key::Return),
            "escape" | "esc" => Some(Key::Escape),
            "a" => Some(Key::KeyA),
            "s" => Some(Key::KeyS),
            "d" => Some(Key::KeyD),
            "f" => Some(Key::KeyF),
            "w" => Some(Key::KeyW),
            "e" => Some(Key::KeyE),
            "r" => Some(Key::KeyR),
            "t" => Some(Key::KeyT),
            "space" => Some(Key::Space),
            _ => None,
        }
    }
}

/// Custom error type for our application
#[derive(Debug)]
#[allow(dead_code)]
enum VimNavError {
    Display(DisplayError),
    Grab(GrabError),
    Simulate(SimulateError),
    Config(ConfigError),
}

impl std::fmt::Display for VimNavError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            VimNavError::Display(e) => write!(f, "Display error: {:?}", e),
            VimNavError::Grab(e) => write!(f, "Grab error: {:?}", e),
            VimNavError::Simulate(e) => write!(f, "Simulate error: {:?}", e),
            VimNavError::Config(e) => write!(f, "Config error: {:?}", e),
        }
    }
}

impl std::error::Error for VimNavError {}

impl From<DisplayError> for VimNavError {
    fn from(err: DisplayError) -> Self {
        VimNavError::Display(err)
    }
}

impl From<GrabError> for VimNavError {
    fn from(err: GrabError) -> Self {
        VimNavError::Grab(err)
    }
}

impl From<SimulateError> for VimNavError {
    fn from(err: SimulateError) -> Self {
        VimNavError::Simulate(err)
    }
}

impl From<ConfigError> for VimNavError {
    fn from(err: ConfigError) -> Self {
        VimNavError::Config(err)
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
    // Modifier tracking
    shift_pressed: bool,
    // Configuration
    config: VimNavConfig,
}

impl CursorState {
    fn new(config: VimNavConfig) -> Result<Self, VimNavError> {
        let (w, h) = display_size()?;
        Ok(CursorState {
            x: w as f64 / 2.0, // start in center
            y: h as f64 / 2.0,
            screen_width: w as f64,
            screen_height: h as f64,
            pressed_keys: HashMap::new(),
            current_speeds: HashMap::new(),
            shift_pressed: false,
            config,
        })
    }

    fn start_key_press(&mut self, key: Key) {
        self.pressed_keys.insert(key, Instant::now());
        self.current_speeds.insert(key, self.config.initial_move_step);
    }

    fn stop_key_press(&mut self, key: Key) {
        self.pressed_keys.remove(&key);
        self.current_speeds.remove(&key);
    }

    fn update_speed(&mut self, key: Key) -> f64 {
        if let Some(start_time) = self.pressed_keys.get(&key) {
            let hold_duration = start_time.elapsed().as_secs_f64();
            
            // Fixed acceleration formula that actually uses the multiplier
            // Formula: speed = initial_move_step + (acceleration_multiplier * acceleration_base ^ hold_duration)
            let exponential_factor = self.config.acceleration_base.powf(hold_duration);
            let new_speed = self.config.initial_move_step + (self.config.acceleration_multiplier * exponential_factor);
            
            // Debug output to see what's happening
            if hold_duration > 0.5 {
                println!("DEBUG: hold_duration={:.2}s, exp_factor={:.2}, multiplier={:.1}, new_speed={:.2}", 
                    hold_duration, exponential_factor, self.config.acceleration_multiplier, new_speed);
            }
            
            // Apply max speed limit only if configured, otherwise unlimited
            let final_speed = match self.config.max_move_step {
                Some(max) => new_speed.min(max),
                None => new_speed, // TRULY UNLIMITED - no safety caps
            };
            
            self.current_speeds.insert(key, final_speed);
            final_speed
        } else {
            self.config.initial_move_step
        }
    }

    fn move_left(&mut self, key: Key) {
        let speed = self.update_speed(key);
        self.x = (self.x - speed).max(0.0);
    }

    fn move_right(&mut self, key: Key) {
        let speed = self.update_speed(key);
        self.x = (self.x + speed).min(self.screen_width - 1.0);
    }

    fn move_up(&mut self, key: Key) {
        let speed = self.update_speed(key);
        self.y = (self.y - speed).max(0.0);
    }

    fn move_down(&mut self, key: Key) {
        let speed = self.update_speed(key);
        self.y = (self.y + speed).min(self.screen_height - 1.0);
    }

    fn is_key_pressed(&self, key: Key) -> bool {
        self.pressed_keys.contains_key(&key)
    }
}

fn send_event(event_type: &EventType, config: &VimNavConfig) -> Result<(), SimulateError> {
    let delay = Duration::from_millis(config.move_delay_ms);
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
    let config = state.config.clone();
    let x = state.x;
    let y = state.y;
    drop(state); // Release lock before sending event
    send_event(&EventType::MouseMove { x, y }, &config)
}

fn click_mouse(config: &VimNavConfig) -> Result<(), SimulateError> {
    // Perform a left mouse click (press and release)
    send_event(&EventType::ButtonPress(Button::Left), config)?;
    send_event(&EventType::ButtonRelease(Button::Left), config)?;
    println!("Mouse clicked!");
    Ok(())
}

fn scroll(direction: &str, config: &VimNavConfig) -> Result<(), SimulateError> {
    let scroll_amount = 3; // Adjust scroll sensitivity
    match direction {
        "up" => {
            for _ in 0..scroll_amount {
                send_event(&EventType::Wheel { delta_x: 0, delta_y: 120 }, config)?;
            }
        },
        "down" => {
            for _ in 0..scroll_amount {
                send_event(&EventType::Wheel { delta_x: 0, delta_y: -120 }, config)?;
            }
        },
        "left" => {
            for _ in 0..scroll_amount {
                send_event(&EventType::Wheel { delta_x: -120, delta_y: 0 }, config)?;
            }
        },
        "right" => {
            for _ in 0..scroll_amount {
                send_event(&EventType::Wheel { delta_x: 120, delta_y: 0 }, config)?;
            }
        },
        _ => {}
    }
    Ok(())
}

fn main() -> Result<(), VimNavError> {
    // Load configuration
    let config = VimNavConfig::load()?;
    config.print_config();
    
    println!("Vim-style navigation with configurable keys started!");
    println!();
    println!("=== CONTROLS ===");
    println!("VIM NAVIGATION MODE:");
    println!("  {} - move cursor left", config.key_left);
    println!("  {} - move cursor down", config.key_down);
    println!("  {} - move cursor up", config.key_up);
    println!("  {} - move cursor right", config.key_right);
    println!("  {} - left mouse click", config.key_click);
    println!("  {} - enter typing mode", config.key_insert_mode);
    println!();
    println!("TYPING MODE:");
    println!("  {} - return to vim navigation mode", config.key_nav_mode);
    println!("  (all other keys work normally for typing)");
    println!();
    println!("BOTH MODES:");
    println!("  Ctrl+C - quit program");
    println!();
    println!("=== STARTING IN VIM NAVIGATION MODE ===");
    println!("Hold movement keys longer for exponential acceleration!");
    println!();

    // Initialize cursor state with config
    let cursor_state = Arc::new(Mutex::new(CursorState::new(config.clone())?));
    
    // Navigation enabled state - true = vim navigation, false = normal typing
    let navigation_enabled = Arc::new(Mutex::new(true));
    
    // Move cursor to initial position
    move_cursor(&cursor_state)?;
    println!("Cursor initialized at center of screen");

    // Create a flag to control the movement thread
    let running = Arc::new(Mutex::new(true));
    
    // Start continuous movement thread
    let cursor_state_movement = Arc::clone(&cursor_state);
    let running_movement = Arc::clone(&running);
    let navigation_enabled_movement = Arc::clone(&navigation_enabled);
    let config_clone = config.clone();
    
    thread::spawn(move || {
        while *running_movement.lock().unwrap() {
            // Only move if navigation is enabled
            if *navigation_enabled_movement.lock().unwrap() {
                let mut state = cursor_state_movement.lock().unwrap();
                let mut moved = false;
                
                let left_key = config_clone.string_to_key(&config_clone.key_left).unwrap_or(Key::KeyH);
                let down_key = config_clone.string_to_key(&config_clone.key_down).unwrap_or(Key::KeyJ);
                let up_key = config_clone.string_to_key(&config_clone.key_up).unwrap_or(Key::KeyK);
                let right_key = config_clone.string_to_key(&config_clone.key_right).unwrap_or(Key::KeyL);
                
                if state.is_key_pressed(left_key) {
                    state.move_left(left_key);
                    moved = true;
                }
                if state.is_key_pressed(down_key) {
                    state.move_down(down_key);
                    moved = true;
                }
                if state.is_key_pressed(up_key) {
                    state.move_up(up_key);
                    moved = true;
                }
                if state.is_key_pressed(right_key) {
                    state.move_right(right_key);
                    moved = true;
                }
                
                if moved {
                    drop(state); // Release the lock before calling move_cursor
                    if let Err(e) = move_cursor(&cursor_state_movement) {
                        eprintln!("Failed to move cursor: {:?}", e);
                    }
                }
            }
            
            thread::sleep(Duration::from_millis(config_clone.repeat_delay_ms));
        }
    });

    // Set up the event listener
    let cursor_state_clone = Arc::clone(&cursor_state);
    let navigation_enabled_clone = Arc::clone(&navigation_enabled);
    let config_clone = config.clone();
    
    let callback = move |event: Event| -> Option<Event> {
        let nav_enabled = *navigation_enabled_clone.lock().unwrap();
        
        match event.event_type {
            EventType::KeyPress(key) => {
                // Track shift state
                if key == Key::ShiftLeft || key == Key::ShiftRight {
                    cursor_state_clone.lock().unwrap().shift_pressed = true;
                }
                
                // Mode switching keys (work in both modes)
                if key == config_clone.string_to_key(&config_clone.key_insert_mode).unwrap_or(Key::KeyI) && nav_enabled {
                    *navigation_enabled_clone.lock().unwrap() = false;
                    // Clear any pressed keys when entering typing mode
                    cursor_state_clone.lock().unwrap().pressed_keys.clear();
                    cursor_state_clone.lock().unwrap().current_speeds.clear();
                    println!("TYPING MODE - navigation disabled");
                    return None; // Block this key
                } else if key == config_clone.string_to_key(&config_clone.key_nav_mode).unwrap_or(Key::Escape) && !nav_enabled {
                    *navigation_enabled_clone.lock().unwrap() = true;
                    println!("VIM NAVIGATION MODE - navigation enabled");
                    return None; // Block this key
                
                // Navigation keys (only work in navigation mode)
                } else if nav_enabled && (
                    key == config_clone.string_to_key(&config_clone.key_left).unwrap_or(Key::KeyH) ||
                    key == config_clone.string_to_key(&config_clone.key_down).unwrap_or(Key::KeyJ) ||
                    key == config_clone.string_to_key(&config_clone.key_up).unwrap_or(Key::KeyK) ||
                    key == config_clone.string_to_key(&config_clone.key_right).unwrap_or(Key::KeyL)
                ) {
                    let shift_pressed = cursor_state_clone.lock().unwrap().shift_pressed;
                    
                    if shift_pressed {
                        // Shift+hjkl = scroll
                        let scroll_dir = match key {
                            k if k == config_clone.string_to_key(&config_clone.key_left).unwrap_or(Key::KeyH) => "left",
                            k if k == config_clone.string_to_key(&config_clone.key_down).unwrap_or(Key::KeyJ) => "down", 
                            k if k == config_clone.string_to_key(&config_clone.key_up).unwrap_or(Key::KeyK) => "up",
                            k if k == config_clone.string_to_key(&config_clone.key_right).unwrap_or(Key::KeyL) => "right",
                            _ => ""
                        };
                        if let Err(e) = scroll(scroll_dir, &config_clone) {
                            eprintln!("Failed to scroll: {:?}", e);
                        }
                    } else {
                        // Normal hjkl = cursor movement
                        cursor_state_clone.lock().unwrap().start_key_press(key);
                    }
                    return None; // Block this key from other apps
                
                // Mouse click (only works in navigation mode)
                } else if nav_enabled && key == config_clone.string_to_key(&config_clone.key_click).unwrap_or(Key::Return) {
                    if let Err(e) = click_mouse(&config_clone) {
                        eprintln!("Failed to click mouse: {:?}", e);
                    }
                    return None; // Block this key
                }
                
                // In navigation mode, let other keys pass through
                // In typing mode, let all keys pass through
                Some(event)
            },
            EventType::KeyRelease(key) => {
                // Track shift state
                if key == Key::ShiftLeft || key == Key::ShiftRight {
                    cursor_state_clone.lock().unwrap().shift_pressed = false;
                }
                
                if nav_enabled && (
                    key == config_clone.string_to_key(&config_clone.key_left).unwrap_or(Key::KeyH) ||
                    key == config_clone.string_to_key(&config_clone.key_down).unwrap_or(Key::KeyJ) ||
                    key == config_clone.string_to_key(&config_clone.key_up).unwrap_or(Key::KeyK) ||
                    key == config_clone.string_to_key(&config_clone.key_right).unwrap_or(Key::KeyL)
                ) {
                    cursor_state_clone.lock().unwrap().stop_key_press(key);
                    return None; // Block this key release too
                }
                
                Some(event)
            },
            _ => Some(event) // Pass through other events
        }
    };

    // Start grabbing events (this will block keys from other apps)
    match grab(callback) {
        Ok(()) => {},
        Err(error) => {
            eprintln!("Error grabbing events: {:?}", error);
            eprintln!("Note: On macOS, make sure the terminal has Accessibility permissions:");
            eprintln!("System Preferences > Security & Privacy > Privacy > Accessibility");
            *running.lock().unwrap() = false;
            return Err(VimNavError::Grab(error));
        }
    }

    Ok(())
}