# Vim Navigation with Toggle Mode & Persistent Configuration

Navigate your computer using vim-style keys (hjkl) to avoid using the mouse and staying on the homerow.

### Config File Setup
```bash
# Copy the example config
cp vim_navigation_config.toml.example vim_navigation_config.toml

# Edit the config file
nvim vim_navigation_config.toml
```

## Requirements

### macOS
- Grant accessibility permissions to Terminal.app
- System Preferences > Security & Privacy > Privacy > Accessibility

### Linux
- Works on X11 systems
- For Wayland: may need `input` group membership

### Windows  
- Should work without special permissions
