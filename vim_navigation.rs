use config::{Config, ConfigError, File};
use rdev::{
    display_size, grab, simulate, Button, DisplayError, Event, EventType, GrabError, Key,
    SimulateError,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

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
    /// Precision mode divisor (how much slower when space is held)
    pub precision_divisor: f64,
    /// Navigation keys
    pub key_left: String,
    pub key_down: String,
    pub key_up: String,
    pub key_right: String,
    pub key_click: String,
    pub key_toggle_mode: String, // Single key to toggle between nav/typing modes
    pub key_right_click: String,
    pub key_select_toggle: String,  // Toggle text selection mode
    pub key_goto_top: String,       // Go to top of screen (gg equivalent)
    pub key_goto_bottom: String,    // Go to bottom of screen (G equivalent)
    pub key_yank: String,           // Copy/yank (y key)
    pub key_paste: String,          // Paste (p key)
}

impl Default for VimNavConfig {
    fn default() -> Self {
        Self {
            initial_move_step: 1.0, // Half the previous speed
            max_move_step: None,    // No speed limit by default!
            acceleration_base: 2.0,
            acceleration_multiplier: 50.0, // Double the multiplier for faster acceleration
            repeat_delay_ms: 30,
            move_delay_ms: 15,
            precision_divisor: 100.0,  // 100x slower by default
            key_left: "h".to_string(),
            key_down: "j".to_string(),
            key_up: "k".to_string(),
            key_right: "l".to_string(),
            key_click: "return".to_string(),
            key_toggle_mode: "escape".to_string(),
            key_right_click: "i".to_string(),
            key_select_toggle: "v".to_string(),
            key_goto_top: "g".to_string(),
            key_goto_bottom: "shift_g".to_string(),
            key_yank: "y".to_string(),
            key_paste: "p".to_string(),
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
            Ok(config) => match config.try_deserialize() {
                Ok(loaded_config) => {
                    println!("Loaded configuration from vim_navigation_config.toml");
                    Ok(loaded_config)
                }
                Err(e) => {
                    println!("Failed to parse config file: {}", e);
                    println!("Using default configuration");
                    Ok(Self::default())
                }
            },
            Err(e) => {
                println!("Failed to build config: {}", e);
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
        println!(
            "Acceleration multiplier: {:.1}",
            self.acceleration_multiplier
        );
        println!("Update rate: {} ms", self.repeat_delay_ms);
        println!("Move delay: {} ms", self.move_delay_ms);
        println!("Precision mode: {:.1}x slower", self.precision_divisor);
        println!(
            "Navigation keys: {} {} {} {} (left/down/up/right)",
            self.key_left, self.key_down, self.key_up, self.key_right
        );
        println!(
            "Control keys: {} (toggle mode), {} (click)",
            self.key_toggle_mode, self.key_click
        );
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
            "g" => Some(Key::KeyG),
            "v" => Some(Key::KeyV),
            "y" => Some(Key::KeyY),
            "p" => Some(Key::KeyP),
            "shift_g" => Some(Key::KeyG), // We'll handle shift detection separately
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
    space_pressed: bool, // For precision mode (100x slower)
    selection_active: bool, // For text selection mode
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
            space_pressed: false,
            selection_active: false,
            config,
        })
    }

    fn start_key_press(&mut self, key: Key) {
        self.pressed_keys.insert(key, Instant::now());
        self.current_speeds
            .insert(key, self.config.initial_move_step);
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
            let mut new_speed = self.config.initial_move_step
                + (self.config.acceleration_multiplier * exponential_factor);

            // Apply precision mode (100x slower) when space is pressed
            if self.space_pressed {
                new_speed /= 10.0;
            }

            // Debug output to see what's happening
            if hold_duration > 0.5 {
                println!("DEBUG: hold_duration={:.2}s, exp_factor={:.2}, multiplier={:.1}, new_speed={:.2}, space_pressed={}", 
                    hold_duration, exponential_factor, self.config.acceleration_multiplier, new_speed, self.space_pressed);
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
        }
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
                send_event(
                    &EventType::Wheel {
                        delta_x: 0,
                        delta_y: 120,
                    },
                    config,
                )?;
            }
        }
        "down" => {
            for _ in 0..scroll_amount {
                send_event(
                    &EventType::Wheel {
                        delta_x: 0,
                        delta_y: -120,
                    },
                    config,
                )?;
            }
        }
        "left" => {
            for _ in 0..scroll_amount {
                send_event(
                    &EventType::Wheel {
                        delta_x: -120,
                        delta_y: 0,
                    },
                    config,
                )?;
            }
        }
        "right" => {
            for _ in 0..scroll_amount {
                send_event(
                    &EventType::Wheel {
                        delta_x: 120,
                        delta_y: 0,
                    },
                    config,
                )?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn right_click_mouse(config: &VimNavConfig) -> Result<(), SimulateError> {
    // Perform a right mouse click (press and release)
    send_event(&EventType::ButtonPress(Button::Right), config)?;
    send_event(&EventType::ButtonRelease(Button::Right), config)?;
    println!("Right mouse clicked!");
    Ok(())
}

fn toggle_selection(cursor_state: &Arc<Mutex<CursorState>>) -> Result<(), SimulateError> {
    let mut state = cursor_state.lock().unwrap();
    state.selection_active = !state.selection_active;
    
    if state.selection_active {
        // Start selection by pressing left mouse button
        simulate(&EventType::ButtonPress(Button::Left))?;
        println!("Text selection started");
    } else {
        // End selection by releasing left mouse button
        simulate(&EventType::ButtonRelease(Button::Left))?;
        println!("Text selection ended");
    }
    Ok(())
}

fn goto_screen_edge(cursor_state: &Arc<Mutex<CursorState>>, go_to_top: bool) -> Result<(), SimulateError> {
    let mut state = cursor_state.lock().unwrap();
    if go_to_top {
        state.y = 0.0;
        println!("Moved to top of screen");
    } else {
        state.y = state.screen_height - 1.0;
        println!("Moved to bottom of screen");
    }
    drop(state);
    
    // Actually move the cursor
    let state = cursor_state.lock().unwrap();
    let x = state.x;
    let y = state.y;
    let config = state.config.clone();
    drop(state);
    
    send_event(&EventType::MouseMove { x, y }, &config)?;
    Ok(())
}

fn yank_copy() -> Result<(), SimulateError> {
    // Send Cmd+C (copy) on macOS
    simulate(&EventType::KeyPress(Key::MetaLeft))?;
    simulate(&EventType::KeyPress(Key::KeyC))?;
    simulate(&EventType::KeyRelease(Key::KeyC))?;
    simulate(&EventType::KeyRelease(Key::MetaLeft))?;
    println!("Yanked (copied) to clipboard");
    Ok(())
}

fn paste() -> Result<(), SimulateError> {
    // Send Cmd+V (paste) on macOS
    simulate(&EventType::KeyPress(Key::MetaLeft))?;
    simulate(&EventType::KeyPress(Key::KeyV))?;
    simulate(&EventType::KeyRelease(Key::KeyV))?;
    simulate(&EventType::KeyRelease(Key::MetaLeft))?;
    println!("Pasted from clipboard");
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
    println!("  {} - right mouse click", config.key_right_click);
    println!("  {} - toggle text selection", config.key_select_toggle);
    println!("  {} - go to top of screen", config.key_goto_top);
    println!("  {} - go to bottom of screen", config.key_goto_bottom);
    println!("  {} - yank/copy", config.key_yank);
    println!("  {} - paste", config.key_paste);
    println!("  Shift+hjkl - scroll in respective directions");
    println!("  Space+hjkl - precision mode ({:.0}x slower)", config.precision_divisor);
    println!("  {} - toggle to typing mode", config.key_toggle_mode);
    println!();
    println!("TYPING MODE:");
    println!(
        "  {} - toggle back to vim navigation mode",
        config.key_toggle_mode
    );
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

    // Move cursor to initial positionnew_speed
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

                let left_key = config_clone
                    .string_to_key(&config_clone.key_left)
                    .unwrap_or(Key::KeyH);
                let down_key = config_clone
                    .string_to_key(&config_clone.key_down)
                    .unwrap_or(Key::KeyJ);
                let up_key = config_clone
                    .string_to_key(&config_clone.key_up)
                    .unwrap_or(Key::KeyK);
                let right_key = config_clone
                    .string_to_key(&config_clone.key_right)
                    .unwrap_or(Key::KeyL);

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
                // Track modifier states
                if key == Key::ShiftLeft || key == Key::ShiftRight {
                    cursor_state_clone.lock().unwrap().shift_pressed = true;
                }
                if key == Key::Space {
                    cursor_state_clone.lock().unwrap().space_pressed = true;
                }

                // Mode switching - single toggle key works in both modes
                if key
                    == config_clone
                        .string_to_key(&config_clone.key_toggle_mode)
                        .unwrap_or(Key::Escape)
                {
                    let mut nav_enabled_guard = navigation_enabled_clone.lock().unwrap();
                    *nav_enabled_guard = !*nav_enabled_guard;
                    if *nav_enabled_guard {
                        println!("VIM NAVIGATION MODE - navigation enabled");
                    } else {
                        println!("TYPING MODE - navigation disabled");
                        // Clear any pressed keys when entering typing mode
                        cursor_state_clone.lock().unwrap().pressed_keys.clear();
                        cursor_state_clone.lock().unwrap().current_speeds.clear();
                    }
                    return None; // Block this key

                // Navigation keys (only work in navigation mode)
                } else if nav_enabled
                    && (key
                        == config_clone
                            .string_to_key(&config_clone.key_left)
                            .unwrap_or(Key::KeyH)
                        || key
                            == config_clone
                                .string_to_key(&config_clone.key_down)
                                .unwrap_or(Key::KeyJ)
                        || key
                            == config_clone
                                .string_to_key(&config_clone.key_up)
                                .unwrap_or(Key::KeyK)
                        || key
                            == config_clone
                                .string_to_key(&config_clone.key_right)
                                .unwrap_or(Key::KeyL))
                {
                    let shift_pressed = cursor_state_clone.lock().unwrap().shift_pressed;

                    if shift_pressed {
                        // Shift+hjkl = scroll
                        let scroll_dir = match key {
                            k if k
                                == config_clone
                                    .string_to_key(&config_clone.key_left)
                                    .unwrap_or(Key::KeyH) =>
                            {
                                "left"
                            }
                            k if k
                                == config_clone
                                    .string_to_key(&config_clone.key_down)
                                    .unwrap_or(Key::KeyJ) =>
                            {
                                "down"
                            }
                            k if k
                                == config_clone
                                    .string_to_key(&config_clone.key_up)
                                    .unwrap_or(Key::KeyK) =>
                            {
                                "up"
                            }
                            k if k
                                == config_clone
                                    .string_to_key(&config_clone.key_right)
                                    .unwrap_or(Key::KeyL) =>
                            {
                                "right"
                            }
                            _ => "",
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
                } else if nav_enabled
                    && key
                        == config_clone
                            .string_to_key(&config_clone.key_click)
                            .unwrap_or(Key::Return)
                {
                    if let Err(e) = click_mouse(&config_clone) {
                        eprintln!("Failed to click mouse: {:?}", e);
                    }
                    return None; // Block this key
                
                // Right mouse click (only works in navigation mode)
                } else if nav_enabled
                    && key
                        == config_clone
                            .string_to_key(&config_clone.key_right_click)
                            .unwrap_or(Key::KeyI)
                {
                    if let Err(e) = right_click_mouse(&config_clone) {
                        eprintln!("Failed to right click mouse: {:?}", e);
                    }
                    return None; // Block this key
                
                // Toggle text selection (only works in navigation mode)
                } else if nav_enabled
                    && key
                        == config_clone
                            .string_to_key(&config_clone.key_select_toggle)
                            .unwrap_or(Key::KeyV)
                {
                    if let Err(e) = toggle_selection(&cursor_state_clone) {
                        eprintln!("Failed to toggle selection: {:?}", e);
                    }
                    return None; // Block this key
                
                // Go to top of screen (only works in navigation mode)
                } else if nav_enabled
                    && key
                        == config_clone
                            .string_to_key(&config_clone.key_goto_top)
                            .unwrap_or(Key::KeyG)
                    && !cursor_state_clone.lock().unwrap().shift_pressed // Plain G, not Shift+G
                {
                    if let Err(e) = goto_screen_edge(&cursor_state_clone, true) {
                        eprintln!("Failed to go to top: {:?}", e);
                    }
                    return None; // Block this key
                
                // Go to bottom of screen (only works in navigation mode)
                } else if nav_enabled
                    && key
                        == config_clone
                            .string_to_key(&config_clone.key_goto_bottom)
                            .unwrap_or(Key::KeyG)
                    && cursor_state_clone.lock().unwrap().shift_pressed // Shift+G
                {
                    if let Err(e) = goto_screen_edge(&cursor_state_clone, false) {
                        eprintln!("Failed to go to bottom: {:?}", e);
                    }
                    return None; // Block this key
                
                // Yank/copy (only works in navigation mode)
                } else if nav_enabled
                    && key
                        == config_clone
                            .string_to_key(&config_clone.key_yank)
                            .unwrap_or(Key::KeyY)
                {
                    if let Err(e) = yank_copy() {
                        eprintln!("Failed to yank/copy: {:?}", e);
                    }
                    return None; // Block this key
                
                // Paste (only works in navigation mode)
                } else if nav_enabled
                    && key
                        == config_clone
                            .string_to_key(&config_clone.key_paste)
                            .unwrap_or(Key::KeyP)
                {
                    if let Err(e) = paste() {
                        eprintln!("Failed to paste: {:?}", e);
                    }
                    return None; // Block this key
                
                // Block space key in navigation mode (used for precision mode)
                } else if nav_enabled && key == Key::Space {
                    return None; // Block space from reaching other apps
                }

                // In navigation mode, let other keys pass through
                // In typing mode, let all keys pass through
                Some(event)
            }
            EventType::KeyRelease(key) => {
                // Track modifier states
                if key == Key::ShiftLeft || key == Key::ShiftRight {
                    cursor_state_clone.lock().unwrap().shift_pressed = false;
                }
                if key == Key::Space {
                    cursor_state_clone.lock().unwrap().space_pressed = false;
                }

                if nav_enabled
                    && (key
                        == config_clone
                            .string_to_key(&config_clone.key_left)
                            .unwrap_or(Key::KeyH)
                        || key
                            == config_clone
                                .string_to_key(&config_clone.key_down)
                                .unwrap_or(Key::KeyJ)
                        || key
                            == config_clone
                                .string_to_key(&config_clone.key_up)
                                .unwrap_or(Key::KeyK)
                        || key
                            == config_clone
                                .string_to_key(&config_clone.key_right)
                                .unwrap_or(Key::KeyL))
                {
                    cursor_state_clone.lock().unwrap().stop_key_press(key);
                    return None; // Block this key release too
                }
                
                // Block space key release in navigation mode
                if nav_enabled && key == Key::Space {
                    return None; // Block space release from reaching other apps
                }

                Some(event)
            }
            _ => Some(event), // Pass through other events
        }
    };

    // Start grabbing events (this will block keys from other apps)
    match grab(callback) {
        Ok(()) => {}
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

