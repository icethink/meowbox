// Windows の release ビルドでコンソールウィンドウを出さない
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    meowbox_desktop_lib::run();
}
