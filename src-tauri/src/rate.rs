//! Đo tốc độ ghi và thời gian còn lại.
//!
//! Ghi ra USB không chảy đều: chép một file nhỏ thì xong tức thì, còn tới lúc
//! ổ đẩy bộ đệm ra bộ nhớ flash thì đứng im vài giây. Lấy hiệu hai mẫu liền
//! nhau sẽ cho ra một con số nhảy loạn giữa 0 và vài trăm MB/s — vô dụng với
//! người đang nhìn màn hình.
//!
//! Nên tốc độ ở đây tính trên một **cửa sổ trượt**: lấy tổng số byte tăng thêm
//! trong vài giây gần nhất chia cho đúng khoảng thời gian đó. Một lần khựng
//! không làm con số rơi về 0, mà một lần chậm hẳn thì vẫn thấy ngay.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Số liệu tiến trình theo byte, gửi thẳng ra giao diện.
///
/// `speed_bps == 0` nghĩa là chưa đo được (mới bắt đầu, hoặc chặng này không
/// đếm byte) — giao diện dựa vào đó để ẩn phần tốc độ đi thay vì hiện "0 B/s".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Throughput {
    pub done: u64,
    pub total: u64,
    pub speed_bps: u64,
    pub eta_secs: u64,
}

/// Ô nhớ dùng-một-lần để gắn số liệu tốc độ vào lần báo tiến trình kế tiếp.
///
/// Các hàm ghi USB báo tiến trình qua một closure `emit(chặng, phần trăm, …)`
/// dùng chung cho hơn ba mươi chỗ gọi, mà chỉ ba trong số đó đếm được byte.
/// Thêm một tham số tốc độ vào closure nghĩa là sửa cả ba mươi chỗ để truyền
/// một giá trị rỗng. Thay vào đó, chặng nào đo được thì đặt số liệu vào đây
/// ngay trước khi gọi `emit`; closure `take()` nó ra và ô nhớ trở lại rỗng, nên
/// số liệu không bao giờ dính sang lần báo sau.
///
/// Dùng `Mutex` chứ không phải `Cell` vì các hàm ghi là `async` và phải `Send`
/// được — `&Cell<T>` thì không `Sync` nên cả future mất `Send`.
#[derive(Default)]
pub struct Slot(std::sync::Mutex<Throughput>);

impl Slot {
    pub fn set(&self, tp: Throughput) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = tp;
    }

    /// Lấy số liệu ra và trả ô nhớ về rỗng.
    pub fn take(&self) -> Throughput {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

/// Cửa sổ trượt để đo tốc độ.
///
/// Ba giây là chỗ cân bằng: đủ dài để nuốt một nhịp khựng của USB, đủ ngắn để
/// con số bám theo thực tế khi tốc độ đổi hẳn (vd chuyển từ chép nhiều file nhỏ
/// sang chép một file 5 GB).
const WINDOW: Duration = Duration::from_secs(3);

/// Dưới ngưỡng này thì khoảng thời gian quá ngắn để chia — sai số của đồng hồ
/// lấn át cả phép đo.
const MIN_SPAN: Duration = Duration::from_millis(400);

pub struct Rate {
    samples: VecDeque<(Instant, u64)>,
    window: Duration,
}

impl Default for Rate {
    fn default() -> Self {
        Self::new()
    }
}

impl Rate {
    pub fn new() -> Self {
        Self { samples: VecDeque::new(), window: WINDOW }
    }

    /// Ghi nhận một mẫu và trả về số liệu tiến trình tại thời điểm đó.
    ///
    /// Nhận `Instant` từ ngoài thay vì tự gọi `now()` để test dựng được một
    /// dòng thời gian giả mà không phải chờ thật.
    pub fn sample(&mut self, at: Instant, done: u64, total: u64) -> Throughput {
        // Bỏ các mẫu đã ra khỏi cửa sổ, nhưng luôn giữ lại ít nhất một mẫu cũ
        // để còn có gì mà trừ.
        while self.samples.len() > 1 {
            let (t, _) = self.samples[0];
            if at.saturating_duration_since(t) > self.window {
                self.samples.pop_front();
            } else {
                break;
            }
        }
        self.samples.push_back((at, done));

        let speed_bps = self.speed(at, done);
        let eta_secs = if speed_bps > 0 && total > done {
            (total - done) / speed_bps
        } else {
            0
        };

        Throughput { done, total, speed_bps, eta_secs }
    }

    fn speed(&self, at: Instant, done: u64) -> u64 {
        let Some(&(first_t, first_done)) = self.samples.front() else { return 0 };
        let span = at.saturating_duration_since(first_t);
        if span < MIN_SPAN {
            return 0;
        }
        // Số đếm chỉ tăng; nếu vì lý do gì đó nó lùi thì coi như chưa đo được
        // còn hơn báo một con số âm bị ép thành số khổng lồ.
        let Some(bytes) = done.checked_sub(first_done) else { return 0 };
        (bytes as f64 / span.as_secs_f64()) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    #[test]
    fn the_slot_hands_its_value_over_exactly_once() {
        // Không xoá sau khi lấy thì mọi lần báo tiến trình sau đó — kể cả các
        // chặng không chép byte nào — đều mang theo tốc độ cũ.
        let slot = Slot::default();
        let tp = Throughput { done: 5, total: 10, speed_bps: 100, eta_secs: 1 };
        slot.set(tp);
        assert_eq!(slot.take(), tp);
        assert_eq!(slot.take(), Throughput::default());
    }

    #[test]
    fn a_steady_stream_reports_its_real_speed() {
        let t0 = Instant::now();
        let mut r = Rate::new();
        // 10 MB/s, lấy mẫu mỗi 500 ms.
        let mut last = Throughput::default();
        for i in 0..=8u64 {
            last = r.sample(at(t0, i * 500), i * 5_000_000, 100_000_000);
        }
        let mbps = last.speed_bps as f64 / 1_000_000.0;
        assert!((9.5..10.5).contains(&mbps), "đo được {mbps} MB/s");
    }

    #[test]
    fn the_first_samples_report_nothing_instead_of_a_wild_guess() {
        // Chưa đủ thời gian để chia thì thà không hiện gì còn hơn hiện một con
        // số bịa ra từ vài chục mili giây đầu.
        let t0 = Instant::now();
        let mut r = Rate::new();
        assert_eq!(r.sample(t0, 0, 1_000).speed_bps, 0);
        assert_eq!(r.sample(at(t0, 100), 500, 1_000).speed_bps, 0);
    }

    #[test]
    fn a_short_stall_does_not_drop_the_speed_to_zero() {
        // Đây là lý do dùng cửa sổ trượt: USB khựng một nhịp khi đẩy bộ đệm,
        // lấy hiệu hai mẫu liền nhau sẽ ra 0 và người dùng tưởng máy treo.
        let t0 = Instant::now();
        let mut r = Rate::new();
        for i in 0..=4u64 {
            r.sample(at(t0, i * 500), i * 5_000_000, 100_000_000);
        }
        // Một giây không nhích được byte nào.
        let tp = r.sample(at(t0, 3_000), 20_000_000, 100_000_000);
        assert!(tp.speed_bps > 3_000_000, "tốc độ tụt hẳn: {} B/s", tp.speed_bps);
    }

    #[test]
    fn a_lasting_slowdown_is_reflected_once_the_window_has_moved_on() {
        let t0 = Instant::now();
        let mut r = Rate::new();
        // Nhanh trong 2 giây đầu…
        for i in 0..=4u64 {
            r.sample(at(t0, i * 500), i * 5_000_000, 200_000_000);
        }
        // …rồi chậm hẳn còn 1 MB/s trong 4 giây kế tiếp.
        let mut last = Throughput::default();
        for i in 1..=8u64 {
            last = r.sample(at(t0, 2_000 + i * 500), 20_000_000 + i * 500_000, 200_000_000);
        }
        let mbps = last.speed_bps as f64 / 1_000_000.0;
        assert!(mbps < 2.0, "cửa sổ chưa trượt qua đoạn nhanh: {mbps} MB/s");
    }

    #[test]
    fn the_remaining_time_comes_from_the_measured_speed() {
        let t0 = Instant::now();
        let mut r = Rate::new();
        for i in 0..=4u64 {
            r.sample(at(t0, i * 500), i * 5_000_000, 100_000_000);
        }
        // Đã ghi 20 MB / 100 MB ở tốc độ 10 MB/s ⇒ còn khoảng 8 giây.
        let tp = r.sample(at(t0, 2_000), 20_000_000, 100_000_000);
        assert!((7..=9).contains(&tp.eta_secs), "còn {} giây", tp.eta_secs);
    }

    #[test]
    fn finishing_leaves_no_remaining_time() {
        let t0 = Instant::now();
        let mut r = Rate::new();
        r.sample(t0, 0, 100);
        let tp = r.sample(at(t0, 1_000), 100, 100);
        assert_eq!(tp.eta_secs, 0);
    }

    #[test]
    fn a_counter_that_goes_backwards_is_ignored_instead_of_overflowing() {
        // Trừ số không dấu mà bị âm sẽ hoá thành một con số khổng lồ, và giao
        // diện sẽ hiện vài exabyte mỗi giây.
        let t0 = Instant::now();
        let mut r = Rate::new();
        r.sample(t0, 50_000_000, 100_000_000);
        let tp = r.sample(at(t0, 1_000), 1_000_000, 100_000_000);
        assert_eq!(tp.speed_bps, 0);
    }
}
