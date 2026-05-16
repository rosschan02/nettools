// 应用入口。`run()` 在 main.rs 中被调用。

mod probe;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        // 后续加新 probe（tcp/dns/http）就在这里追加。
        .invoke_handler(tauri::generate_handler![
            probe::ping::ping_host,
            probe::ping::ping_backend,
            probe::tcp::tcp_probe,
            probe::tcp::tcp_scan,
            probe::tcp::tcp_ping,
            probe::dns::dns_query,
            probe::http::http_request,
            probe::traceroute::traceroute_run,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
