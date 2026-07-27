// 릴리스 빌드에서 콘솔 창이 뜨지 않게 한다. 상주 앱이므로 반드시 유지할 것.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    claude_usage_widget_lib::run()
}
