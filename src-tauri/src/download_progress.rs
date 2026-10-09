//! Cálculo del progreso global de descarga: velocidad media y tiempo restante (ETA).
//!
//! Estructura pura y sin dependencias de la interfaz: acumula los bytes descargados y calcula
//! la velocidad media sobre una ventana temporal de 5 s, además del ETA a partir de ella.

use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

/// Ventana temporal sobre la que se calcula la velocidad media.
pub const SPEED_WINDOW: Duration = Duration::from_secs(5);

/// Instantánea del progreso global en un momento concreto.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DownloadProgressSnapshot {
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub speed_bytes_per_sec: f64,
    pub eta_seconds: Option<u64>,
}

/// Acumula los bytes descargados y deriva velocidad media y ETA.
#[derive(Debug)]
pub struct DownloadProgress {
    bytes_total: u64,
    bytes_done: u64,
    samples: VecDeque<(Instant, u64)>,
}

impl DownloadProgress {
    /// Crea el acumulador con el total de bytes pendientes de descargar.
    pub fn new(bytes_total: u64) -> Self {
        Self {
            bytes_total,
            bytes_done: 0,
            samples: VecDeque::new(),
        }
    }

    /// Registra `bytes` descargados en el instante `now`.
    pub fn record(&mut self, bytes: u64, now: Instant) {
        self.bytes_done = self.bytes_done.saturating_add(bytes);
        // Un reintento puede recontar bytes ya descargados: nunca sobrepasamos el total.
        if self.bytes_total > 0 {
            self.bytes_done = self.bytes_done.min(self.bytes_total);
        }
        self.discard_expired_samples(now);
        self.samples.push_back((now, self.bytes_done));
    }

    #[cfg(test)]
    pub fn bytes_done(&self) -> u64 {
        self.bytes_done
    }

    /// Velocidad media (bytes por segundo) de los últimos [`SPEED_WINDOW`].
    pub fn speed_bytes_per_sec(&self, now: Instant) -> f64 {
        let cutoff = now.checked_sub(SPEED_WINDOW);
        let reference = match cutoff {
            Some(cutoff) => self
                .samples
                .iter()
                .find(|(at, _)| *at >= cutoff)
                .copied(),
            // `now` está tan cerca del origen que la ventana completa queda por delante.
            None => self.samples.front().copied(),
        };
        let Some((at, bytes)) = reference else {
            return 0.0;
        };
        let elapsed = now.saturating_duration_since(at);
        if elapsed.is_zero() {
            return 0.0;
        }
        let downloaded = self.bytes_done.saturating_sub(bytes);
        downloaded as f64 / elapsed.as_secs_f64()
    }

    /// Instantánea actual con velocidad y ETA (`None` si aún no hay velocidad calculable).
    pub fn snapshot(&self, now: Instant) -> DownloadProgressSnapshot {
        let speed_bytes_per_sec = self.speed_bytes_per_sec(now);
        let remaining = self.bytes_total.saturating_sub(self.bytes_done);
        let eta_seconds = if speed_bytes_per_sec > 0.0 {
            Some((remaining as f64 / speed_bytes_per_sec).ceil() as u64)
        } else {
            None
        };
        DownloadProgressSnapshot {
            bytes_done: self.bytes_done,
            bytes_total: self.bytes_total,
            speed_bytes_per_sec,
            eta_seconds,
        }
    }

    fn discard_expired_samples(&mut self, now: Instant) {
        while let Some(&(at, _)) = self.samples.front() {
            if now.saturating_duration_since(at) > SPEED_WINDOW {
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(base: Instant, seconds: u64) -> Instant {
        base + Duration::from_secs(seconds)
    }

    #[test]
    fn without_data_speed_and_eta_are_zero() {
        let progress = DownloadProgress::new(1024);
        let snapshot = progress.snapshot(Instant::now());
        assert_eq!(snapshot.bytes_done, 0);
        assert_eq!(snapshot.bytes_total, 1024);
        assert_eq!(snapshot.speed_bytes_per_sec, 0.0);
        assert_eq!(snapshot.eta_seconds, None);
    }

    #[test]
    fn a_single_sample_reports_zero_speed() {
        let base = Instant::now();
        let mut progress = DownloadProgress::new(10_000);
        progress.record(1_000, base);
        let snapshot = progress.snapshot(base);
        assert_eq!(snapshot.speed_bytes_per_sec, 0.0);
        assert_eq!(snapshot.eta_seconds, None);
    }

    #[test]
    fn speed_uses_only_the_last_five_seconds() {
        let base = Instant::now();
        let mut progress = DownloadProgress::new(10_000_000);
        progress.record(1_000_000, at(base, 0));
        progress.record(1_000_000, at(base, 5));
        progress.record(1_000_000, at(base, 10));

        // La muestra de 0 s ya queda fuera de la ventana; solo cuentan de 5 s a 10 s.
        let speed = progress.speed_bytes_per_sec(at(base, 10));
        assert!((speed - 200_000.0).abs() < 1.0, "velocidad inesperada: {speed}");
    }

    #[test]
    fn eta_is_derived_from_the_current_speed() {
        let base = Instant::now();
        let mut progress = DownloadProgress::new(10_000_000);
        progress.record(1_000_000, at(base, 0));
        progress.record(1_000_000, at(base, 1));

        let snapshot = progress.snapshot(at(base, 1));
        assert!((snapshot.speed_bytes_per_sec - 1_000_000.0).abs() < 1.0);
        // Faltan 8 MB a 1 MB/s: 8 s.
        assert_eq!(snapshot.eta_seconds, Some(8));
    }

    #[test]
    fn zero_elapsed_time_does_not_divide_by_zero() {
        let base = Instant::now();
        let mut progress = DownloadProgress::new(1_000);
        progress.record(100, base);
        progress.record(100, base);

        let snapshot = progress.snapshot(base);
        assert_eq!(snapshot.speed_bytes_per_sec, 0.0);
        assert_eq!(snapshot.eta_seconds, None);
    }

    #[test]
    fn recorded_bytes_never_exceed_the_total() {
        let base = Instant::now();
        let mut progress = DownloadProgress::new(100);
        progress.record(80, at(base, 0));
        progress.record(80, at(base, 1));

        assert_eq!(progress.bytes_done(), 100);
    }
}
