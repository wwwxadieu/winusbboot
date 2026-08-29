// Ẩn cửa sổ console đen khi chạy bản release trên Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    getwinusb_lib::run()
}
