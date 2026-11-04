// === 0. Native 초기 런타임 설정: 별도 민감 변수를 포함하지 않습니다 ===
// 실제 실파일 저장, 시스템 상태 및 멀티윈도우 명령은 보안 설계 후 추가합니다.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("MemoNest native application failed to start");
}
