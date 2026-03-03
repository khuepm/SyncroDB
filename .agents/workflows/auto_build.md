---
description: Tự động cài đặt thư viện và build ứng dụng Tauri thành file thực thi
---

Khi tôi được yêu cầu chạy lệnh này (ví dụ: gõ `/auto_build` hoặc "build app"), hãy tự động thực hiện các bước sau để xuất bản ứng dụng Tauri:

// turbo-all
1. Cài đặt các thư viện liên quan (nếu chưa cài đặt)
```bash
npm install
```

2. Tiến hành build ứng dụng Tauri (sẽ tự build giao diện React và backend Rust để tạo ra file thực thi)
```bash
npm run tauri build
```
