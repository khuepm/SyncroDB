---
description: Tự động commit và đẩy code lên git sau khi sửa lỗi (Fix bug)
---

Khi tôi yêu cầu chạy workflow hoặc khi sửa xong một lỗi, hãy thực hiện lần lượt các bước sau để lưu trữ:

// turbo-all
1. Thêm tất cả các file đã thay đổi vào Git
```bash
git add .
```

2. Tạo một commit tự động với nội dung mô tả ngắn gọn thay đổi (có thể đặt tự động theo ngữ cảnh).
```bash
git commit -m "chore: auto-sync sau khi fix bug/task"
```

3. Đẩy lên nhánh hiện tại trên Git (Push)
```bash
git push
```
