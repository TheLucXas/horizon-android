//! Original installer UI strings and download telemetry, independent of Minecraft language files.
use horizon_client::official_downloads::Progress;
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug)]
pub(super) enum Status {
    Waiting,
    RuntimePreparing,
    GamePreparing,
    Sounds,
}
#[derive(Clone, Copy, Debug)]
pub(super) enum Event {
    Status(Status),
    Download(Progress),
}

#[derive(Default)]
struct Rate {
    samples: VecDeque<(Instant, u64)>,
    bytes: u64,
    last_network: Option<Instant>,
}
impl Rate {
    fn observe(&mut self, bytes: u64, now: Instant) {
        if bytes <= self.bytes {
            return;
        }
        if self
            .last_network
            .is_some_and(|last| now.saturating_duration_since(last) > Duration::from_secs(3))
        {
            self.samples.clear();
        }
        if self.samples.is_empty() {
            self.samples.push_back((now, self.bytes));
        }
        self.bytes = bytes;
        self.last_network = Some(now);
        while self.samples.len() > 2
            && now.saturating_duration_since(self.samples[1].0) >= Duration::from_secs(5)
        {
            self.samples.pop_front();
        }
        if now.saturating_duration_since(self.samples.back().unwrap().0)
            >= Duration::from_millis(250)
        {
            self.samples.push_back((now, bytes));
        }
    }
    fn per_second(&self, now: Instant) -> Option<f64> {
        let last = self.last_network?;
        if now.saturating_duration_since(last) > Duration::from_secs(3) {
            return Some(0.);
        }
        let (start, bytes) = *self.samples.front()?;
        let elapsed = now.saturating_duration_since(start).as_secs_f64();
        (elapsed >= 0.25).then(|| self.bytes.saturating_sub(bytes) as f64 / elapsed)
    }
}

pub(super) struct InstallProgress {
    language: &'static str,
    event: Event,
    rate: Rate,
}
impl InstallProgress {
    pub(super) fn new(android_tag: &str) -> Self {
        let language = match android_tag
            .split(['-', '_'])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "es" => "es",
            "pt" => "pt",
            _ => "en",
        };
        Self {
            language,
            event: Event::Status(Status::Waiting),
            rate: Rate::default(),
        }
    }
    pub(super) fn update(&mut self, event: Event, now: Instant) {
        let reset = match (self.event, event) {
            (Event::Download(old), Event::Download(new)) => {
                old.stage != new.stage
                    || old.total != new.total
                    || new.network_bytes < old.network_bytes
            }
            _ => true,
        };
        if reset {
            self.rate = Rate::default();
        }
        if let Event::Download(p) = event {
            self.rate.observe(p.network_bytes, now);
        }
        self.event = event;
    }
    pub(super) fn text(&self, key: &str) -> &'static str {
        text(self.language, key)
    }
    fn number(&self, value: f64, decimals: usize) -> String {
        let result = format!("{value:.decimals$}");
        if self.language == "en" {
            result
        } else {
            result.replace('.', ",")
        }
    }
    fn amount(&self, bytes: f64) -> String {
        if bytes >= 1_000_000_000. {
            format!("{} GB", self.number(bytes / 1_000_000_000., 2))
        } else {
            format!("{} MB", self.number(bytes / 1_000_000., 1))
        }
    }
    fn speed(&self, bytes_per_second: f64) -> String {
        if bytes_per_second > 0. && bytes_per_second < 1_000. {
            format!("<{} MB/s", self.number(0.001, 3))
        } else if (1_000.0..1_000_000.0).contains(&bytes_per_second) {
            format!("{} MB/s", self.number(bytes_per_second / 1_000_000., 3))
        } else {
            format!("{}/s", self.amount(bytes_per_second))
        }
    }
    pub(super) fn fraction(&self) -> Option<f32> {
        match self.event {
            Event::Download(p) if p.total > 0 => {
                Some((p.completed as f64 / p.total as f64).clamp(0., 1.) as f32)
            }
            _ => None,
        }
    }
    pub(super) fn lines(&self, now: Instant) -> Vec<String> {
        let p = match self.event {
            Event::Status(status) => {
                return vec![
                    self.text(match status {
                        Status::Waiting => "waiting",
                        Status::RuntimePreparing => "runtime_prepare",
                        Status::GamePreparing => "game_prepare",
                        Status::Sounds => "sounds",
                    })
                    .into(),
                ];
            }
            Event::Download(p) => p,
        };
        let key = match p.stage {
            "Manifiesto" => "manifest",
            "Cliente" => "client",
            "Servidor" => "server",
            "Bibliotecas" => "libraries",
            "Índice de recursos" => "index",
            "Recursos" => "resources",
            "OpenJDK" => "runtime",
            _ => "game_prepare",
        };
        let mut lines = vec![self.text(key).into()];
        if p.stage == "Manifiesto" {
            return lines;
        }
        let amount = self.amount(p.completed as f64);
        lines.push(if p.total > 0 {
            format!(
                "{} / {}  ({}%)",
                amount,
                self.amount(p.total as f64),
                (self.fraction().unwrap_or(0.) * 100.).floor() as u32
            )
        } else {
            format!("{} · {}", amount, self.text("unknown_total"))
        });
        let rate = self.rate.per_second(now);
        let speed = rate
            .map(|n| self.speed(n))
            .unwrap_or_else(|| self.text("calculating").into());
        let eta = if p.total > 0 && p.completed >= p.total {
            time(0)
        } else if p.total > 0 {
            rate.filter(|&n| n > 0.)
                .map(|n| time(((p.total.saturating_sub(p.completed) as f64 / n).ceil()) as u64))
                .unwrap_or_else(|| self.text("calculating").into())
        } else {
            self.text("calculating").into()
        };
        lines.push(format!("{}: {}", self.text("speed"), speed));
        lines.push(format!("{}: {}", self.text("eta"), eta));
        lines
    }
    pub(super) fn error(&self, detail: &str) -> String {
        let lower = detail.to_lowercase();
        let key = if lower.contains("enlace simbólico") {
            "error_symlink"
        } else if lower.contains("no space") || lower.contains("os error 28") {
            "error_space"
        } else if lower.contains("verificada") || lower.contains("cancelada") {
            "error_account"
        } else if lower.contains("descargar")
            || lower.contains("interrump")
            || lower.contains("conexión")
        {
            "error_network"
        } else if lower.contains("sha")
            || lower.contains("hash")
            || lower.contains("manifiesto")
            || lower.contains("metadatos")
            || lower.contains("dañado")
        {
            "error_integrity"
        } else if lower.contains("permission denied")
            || lower.contains("caché")
            || lower.contains("guardar")
        {
            "error_storage"
        } else {
            "error_failed"
        };
        self.text(key).into()
    }
}
fn time(seconds: u64) -> String {
    if seconds >= 3600 {
        format!("{} h {} min", seconds / 3600, seconds % 3600 / 60)
    } else if seconds >= 60 {
        format!("{} min {} s", seconds / 60, seconds % 60)
    } else {
        format!("{seconds} s")
    }
}
fn text(language: &str, key: &str) -> &'static str {
    let (en, es, pt) = match key {
        "installation" => (
            "Horizon · Installation",
            "Horizon · Instalación",
            "Horizon · Instalação",
        ),
        "retry" => ("Retry", "Reintentar", "Tentar novamente"),
        "waiting" => (
            "Verifying your Minecraft account…",
            "Verificando tu cuenta de Minecraft…",
            "Verificando sua conta Minecraft…",
        ),
        "manifest" => (
            "Loading the download list…",
            "Cargando la lista de descargas…",
            "Carregando a lista de downloads…",
        ),
        "client" => (
            "Downloading the game client",
            "Descargando el cliente del juego",
            "Baixando o cliente do jogo",
        ),
        "server" => (
            "Downloading server files",
            "Descargando archivos del servidor",
            "Baixando arquivos do servidor",
        ),
        "libraries" => (
            "Downloading libraries",
            "Descargando bibliotecas",
            "Baixando bibliotecas",
        ),
        "index" => (
            "Downloading the resource index",
            "Descargando el índice de recursos",
            "Baixando o índice de recursos",
        ),
        "resources" => (
            "Downloading resources",
            "Descargando recursos",
            "Baixando recursos",
        ),
        "runtime" => (
            "Downloading OpenJDK 25",
            "Descargando OpenJDK 25",
            "Baixando o OpenJDK 25",
        ),
        "runtime_prepare" => (
            "Unpacking OpenJDK 25…",
            "Descomprimiendo OpenJDK 25…",
            "Descompactando o OpenJDK 25…",
        ),
        "game_prepare" => (
            "Preparing game files…",
            "Preparando archivos del juego…",
            "Preparando arquivos do jogo…",
        ),
        "sounds" => (
            "Installing sounds and languages…",
            "Instalando sonidos e idiomas…",
            "Instalando sons e idiomas…",
        ),
        "unknown_total" => (
            "total not yet available",
            "total aún no disponible",
            "total ainda indisponível",
        ),
        "calculating" => ("calculating…", "calculando…", "calculando…"),
        "speed" => ("Speed", "Velocidad", "Velocidade"),
        "eta" => (
            "ETA for this download",
            "Tiempo restante de esta descarga",
            "Tempo restante deste download",
        ),
        "error_symlink" => (
            "A symlink inside the cache was blocked. Installation stopped.",
            "Se bloqueó un enlace simbólico dentro de la caché. Instalación detenida.",
            "Um link simbólico no cache foi bloqueado. Instalação interrompida.",
        ),
        "error_space" => (
            "Not enough storage space. Free some space and retry.",
            "No hay espacio suficiente. Libera espacio y reintenta.",
            "Espaço insuficiente. Libere espaço e tente novamente.",
        ),
        "error_account" => (
            "Sign in again to continue installation.",
            "Inicia sesión de nuevo para continuar la instalación.",
            "Entre novamente para continuar a instalação.",
        ),
        "error_network" => (
            "The download was interrupted. Check the connection and retry.",
            "Se interrumpió la descarga. Comprueba la conexión y reintenta.",
            "O download foi interrompido. Verifique a conexão e tente novamente.",
        ),
        "error_integrity" => (
            "File verification failed. Retry the download.",
            "Falló la verificación de archivos. Reintenta la descarga.",
            "A verificação dos arquivos falhou. Tente baixar novamente.",
        ),
        "error_storage" => (
            "Could not write installation files. Check storage and retry.",
            "No se pudieron guardar los archivos. Comprueba el almacenamiento y reintenta.",
            "Não foi possível salvar os arquivos. Verifique o armazenamento e tente novamente.",
        ),
        _ => (
            "Could not prepare game files. Retry installation.",
            "No se pudieron preparar los archivos. Reintenta la instalación.",
            "Não foi possível preparar os arquivos. Tente instalar novamente.",
        ),
    };
    match language {
        "es" => es,
        "pt" => pt,
        _ => en,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn download(done: u64, total: u64, network: u64) -> Event {
        Event::Download(Progress {
            stage: "Recursos",
            completed: done,
            total,
            network_bytes: network,
        })
    }
    #[test]
    fn cached_bytes_do_not_create_speed_or_eta_and_network_has_units() {
        let start = Instant::now();
        let mut p = InstallProgress::new("es-AR");
        p.update(download(400_000_000, 1_200_000_000, 0), start);
        assert!(p.lines(start)[1].contains("400,0 MB / 1,20 GB"));
        assert!(p.lines(start)[2].contains("calculando"));
        p.update(download(401_000_000, 1_200_000_000, 1_000_000), start);
        p.update(
            download(403_000_000, 1_200_000_000, 3_000_000),
            start + Duration::from_secs(2),
        );
        assert!(p.lines(start + Duration::from_secs(2))[2].contains("1,5 MB/s"));
        assert!(!p.lines(start + Duration::from_secs(2))[3].contains("calculando"));
        assert!(p.lines(start + Duration::from_secs(6))[2].contains("0,0 MB/s"));
        assert!(p.lines(start + Duration::from_secs(6))[3].contains("calculando"));
    }
    #[test]
    fn unknown_total_and_new_stage_have_no_stale_eta() {
        let t = Instant::now();
        let mut p = InstallProgress::new("fr-FR");
        assert_eq!(p.text("installation"), "Horizon · Installation");
        p.update(download(0, 0, 0), t);
        assert!(p.lines(t)[1].contains("not yet available"));
        p.update(Event::Status(Status::GamePreparing), t);
        assert_eq!(p.lines(t), vec!["Preparing game files…"]);
        assert!(p.fraction().is_none());
    }
    #[test]
    fn language_units_completion_and_errors() {
        let t = Instant::now();
        let mut p = InstallProgress::new("pt-BR");
        assert_eq!(p.text("retry"), "Tentar novamente");
        assert_eq!(p.speed(10_000.), "0,010 MB/s");
        assert_eq!(p.speed(500.), "<0,001 MB/s");
        assert_eq!(p.amount(999_000_000.), "999,0 MB");
        assert_eq!(p.amount(1_000_000_000.), "1,00 GB");
        p.update(download(4, 4, 0), t);
        assert!(p.lines(t)[3].ends_with("0 s"));
        assert!(
            p.error("Enlace simbólico en la caché")
                .contains("link simbólico")
        );
        assert_eq!(time(3661), "1 h 1 min");
    }
}
