// No console window behind the app on Windows, but in development.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    open_bus_lib::run();
}
