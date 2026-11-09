// 데스크톱 진입점. 모바일 빌드에서는 lib.rs의 run()을 사용합니다.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() {
    memonest_lib::run();
}
