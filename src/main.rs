use iced::widget::{button, column, container, horizontal_space, row, scrollable, slider, text};
use iced::{time, Alignment, Background, Border, Color, Element, Length, Shadow, Subscription, Task, Theme, Vector};
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::{
    fs::File,
    io::{BufReader, Read},
    path::{Path, PathBuf},
    process::{Child, ChildStdout, Command, Stdio},
    time::Duration,
};

const MEDIA: &[&str] = &["mp3", "flac", "wav", "ogg", "m4a", "aac", "wma"];

// M2.5 の配色（設定画面と共通）
const BG: u32 = 0xf8f9fa;
const TEXT: u32 = 0x1f1f1f;
const MUTED: u32 = 0x5f6368;
const ACCENT: u32 = 0x0b57d0;
const TONAL: u32 = 0xd3e3fd;
const TONAL_TEXT: u32 = 0x041e49;
const FAB: u32 = 0xf2a6a6;
const HOVER: u32 = 0xeef0f2;

fn rgb(hex: u32) -> Color {
    Color::from_rgb8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

// ---- WMA 用: ffmpeg の PCM 出力を直接読む（一時ファイルは作らない） ----

struct FfmpegSource {
    child: Child,
    out: BufReader<ChildStdout>,
}

impl FfmpegSource {
    fn open(path: &Path) -> Option<Self> {
        let mut child = Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(path)
            .args(["-f", "s16le", "-ac", "2", "-ar", "44100", "-"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let out = BufReader::new(child.stdout.take()?);
        Some(Self { child, out })
    }
}

impl Iterator for FfmpegSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let mut bytes = [0u8; 2];
        self.out.read_exact(&mut bytes).ok()?;
        Some(i16::from_le_bytes(bytes) as f32 / 32768.0)
    }
}

impl Source for FfmpegSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        2
    }
    fn sample_rate(&self) -> u32 {
        44100
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

impl Drop for FfmpegSource {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn is_wma(path: &Path) -> bool {
    matches!(ext(path).as_deref(), Some("wma" | "wmv"))
}

fn is_media(path: &Path) -> bool {
    ext(path).is_some_and(|e| MEDIA.contains(&e.as_str()))
}

fn ext(path: &Path) -> Option<String> {
    path.extension().map(|e| e.to_string_lossy().to_lowercase())
}

fn fmt(s: u32) -> String {
    format!("{}:{:02}", s / 60, s % 60)
}

async fn pick_files() -> Vec<PathBuf> {
    rfd::AsyncFileDialog::new()
        .add_filter("メディア", MEDIA)
        .pick_files()
        .await
        .map(|files| files.into_iter().map(|f| f.path().to_path_buf()).collect())
        .unwrap_or_default()
}

// ---- アプリ本体 ----

#[derive(Debug, Clone)]
enum Msg {
    OpenFiles,
    FilesPicked(Vec<PathBuf>),
    Select(usize),
    Toggle,
    Next,
    Prev,
    Stop,
    Volume(f32),
    Tick,
}

struct App {
    // 出力先は保持しておく必要がある（drop されると音が止まる）
    audio: Option<(OutputStream, OutputStreamHandle)>,
    sink: Option<Sink>,
    playlist: Vec<PathBuf>,
    index: Option<usize>,
    volume: f32,
    elapsed: u32,
    status: String,
}

impl App {
    fn new() -> Self {
        let audio = OutputStream::try_default().ok();
        let status = if audio.is_none() {
            "オーディオ出力を開けません".into()
        } else {
            String::new()
        };
        Self {
            audio,
            sink: None,
            playlist: Vec::new(),
            index: None,
            volume: 0.7,
            elapsed: 0,
            status,
        }
    }

    fn playing(&self) -> bool {
        self.sink.as_ref().is_some_and(|s| !s.is_paused() && !s.empty())
    }

    fn play(&mut self, i: usize) {
        let Some((_, handle)) = &self.audio else {
            self.status = "オーディオ出力を開けません".into();
            return;
        };
        let path = self.playlist[i].clone();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();

        if let Some(old) = self.sink.take() {
            old.stop();
        }
        let sink = match Sink::try_new(handle) {
            Ok(sink) => sink,
            Err(_) => {
                self.status = "オーディオ出力を開けません".into();
                return;
            }
        };

        let started = if is_wma(&path) {
            FfmpegSource::open(&path)
                .map(|src| sink.append(src))
                .is_some()
        } else {
            File::open(&path)
                .ok()
                .and_then(|f| Decoder::new(BufReader::new(f)).ok())
                .map(|d| sink.append(d))
                .is_some()
        };

        if started {
            sink.set_volume(self.volume);
            self.sink = Some(sink);
            self.index = Some(i);
            self.elapsed = 0;
            self.status.clear();
        } else {
            self.status = format!("再生できません: {name}");
        }
    }

    fn next(&mut self) {
        match self.index {
            Some(i) if i + 1 < self.playlist.len() => self.play(i + 1),
            _ => self.stop(),
        }
    }

    fn prev(&mut self) {
        if let Some(i) = self.index {
            // 3秒以上再生していれば曲の頭に戻す
            if self.elapsed > 3 || i == 0 {
                self.play(i);
            } else {
                self.play(i - 1);
            }
        }
    }

    fn stop(&mut self) {
        if let Some(s) = self.sink.take() {
            s.stop();
        }
        self.elapsed = 0;
    }

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::OpenFiles => return Task::perform(pick_files(), Msg::FilesPicked),
            Msg::FilesPicked(files) => {
                let first_new = self.playlist.len();
                self.playlist.extend(files.into_iter().filter(|p| is_media(p)));
                if self.index.is_none() && self.playlist.len() > first_new {
                    self.play(first_new);
                }
            }
            Msg::Select(i) => self.play(i),
            Msg::Toggle => {
                if let Some(s) = &self.sink {
                    if s.is_paused() {
                        s.play();
                    } else {
                        s.pause();
                    }
                } else if !self.playlist.is_empty() {
                    self.play(self.index.unwrap_or(0));
                }
            }
            Msg::Next => self.next(),
            Msg::Prev => self.prev(),
            Msg::Stop => self.stop(),
            Msg::Volume(v) => {
                self.volume = v;
                if let Some(s) = &self.sink {
                    s.set_volume(v);
                }
            }
            Msg::Tick => {
                if self.playing() {
                    self.elapsed += 1;
                }
                // 曲が最後まで流れたら次へ進む
                if self.sink.as_ref().is_some_and(|s| s.empty() && !s.is_paused()) {
                    self.next();
                }
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Msg> {
        let appbar = container(
            row![
                text("Aurore Player").size(20),
                horizontal_space(),
                button(text("ファイルを開く").size(13))
                    .on_press(Msg::OpenFiles)
                    .padding([10.0, 24.0])
                    .style(tonal_style),
            ]
            .align_y(Alignment::Center),
        )
        .padding([12.0, 20.0])
        .width(Length::Fill)
        .style(card_style);

        // プレイリスト
        let mut list = column![].spacing(4);
        for (i, path) in self.playlist.iter().enumerate() {
            let selected = self.index == Some(i);
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            list = list.push(
                button(text(name).size(13))
                    .on_press(Msg::Select(i))
                    .width(Length::Fill)
                    .padding([10.0, 12.0])
                    .style(move |_, s| item_style(s, selected)),
            );
        }
        let playlist = container(
            column![
                text("プレイリスト").size(13).color(rgb(ACCENT)),
                scrollable(list).height(Length::Fill),
            ]
            .spacing(12),
        )
        .padding(16)
        .width(Length::Fixed(260.0))
        .height(Length::Fill)
        .style(card_style);

        // プレイヤー
        let now = self
            .index
            .and_then(|i| self.playlist.get(i))
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "再生していません".into());

        let state = if self.playing() {
            "再生中"
        } else if self.sink.is_some() {
            "一時停止"
        } else {
            "停止"
        };

        let controls = row![
            icon_btn("⏮", Msg::Prev),
            button(text(if self.playing() { "⏸" } else { "▶" }).size(26))
                .on_press(Msg::Toggle)
                .width(Length::Fixed(64.0))
                .height(Length::Fixed(64.0))
                .style(fab_style),
            icon_btn("⏭", Msg::Next),
            icon_btn("■", Msg::Stop),
        ]
        .spacing(12)
        .align_y(Alignment::Center);

        let volume = row![
            text("音量").size(12).color(rgb(MUTED)),
            slider(0.0..=1.0, self.volume, Msg::Volume)
                .step(0.01)
                .width(Length::Fixed(160.0)),
        ]
        .spacing(12)
        .align_y(Alignment::Center);

        let player = container(
            column![
                text(now).size(16),
                text(format!("{} ・ {}", fmt(self.elapsed), state))
                    .size(12)
                    .color(rgb(MUTED)),
                container(controls).center_x(Length::Fill),
                row![horizontal_space(), volume],
            ]
            .spacing(16),
        )
        .padding(20)
        .width(Length::Fill)
        .style(card_style);

        let mut body = column![row![playlist, player].spacing(16)]
            .padding(16)
            .width(Length::Fill)
            .height(Length::Fill);
        if !self.status.is_empty() {
            body = body.push(text(self.status.as_str()).size(12).color(rgb(0xb3261e)));
        }

        column![appbar, body].into()
    }

    fn subscription(&self) -> Subscription<Msg> {
        time::every(Duration::from_secs(1)).map(|_| Msg::Tick)
    }

    fn theme(&self) -> Theme {
        Theme::custom(
            "Aurore".to_string(),
            iced::theme::Palette {
                background: rgb(BG),
                text: rgb(TEXT),
                primary: rgb(ACCENT),
                success: rgb(ACCENT),
                danger: rgb(0xb3261e),
            },
        )
    }
}

// ---- スタイル ----

fn card_style(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(rgb(0xffffff))),
        border: Border {
            radius: 16.0.into(),
            ..Default::default()
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.16),
            offset: Vector::new(0.0, 2.0),
            blur_radius: 16.0,
        },
        ..Default::default()
    }
}

fn tonal_style(_: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered | button::Status::Pressed => 0xc2d7fb,
        _ => TONAL,
    };
    button::Style {
        background: Some(Background::Color(rgb(bg))),
        text_color: rgb(TONAL_TEXT),
        border: Border {
            radius: 999.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn fab_style(_: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => 0xe89494,
        button::Status::Pressed => 0xde8282,
        _ => FAB,
    };
    button::Style {
        background: Some(Background::Color(rgb(bg))),
        text_color: rgb(TEXT),
        border: Border {
            radius: 32.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn icon_btn<'a>(label: &'static str, msg: Msg) -> Element<'a, Msg> {
    button(text(label).size(18))
        .on_press(msg)
        .width(Length::Fixed(48.0))
        .height(Length::Fixed(48.0))
        .style(icon_style)
        .into()
}

fn icon_style(_: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => Some(Background::Color(rgb(HOVER))),
        _ => None,
    };
    button::Style {
        background,
        text_color: rgb(TEXT),
        border: Border {
            radius: 24.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn item_style(status: button::Status, selected: bool) -> button::Style {
    let (background, text_color) = if selected {
        (Some(Background::Color(rgb(TONAL))), rgb(TONAL_TEXT))
    } else {
        match status {
            button::Status::Hovered | button::Status::Pressed => {
                (Some(Background::Color(rgb(HOVER))), rgb(TEXT))
            }
            _ => (None, rgb(TEXT)),
        }
    };
    button::Style {
        background,
        text_color,
        border: Border {
            radius: 12.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn main() -> iced::Result {
    iced::application("Aurore Player", App::update, App::view)
        .theme(App::theme)
        .subscription(App::subscription)
        .window_size(iced::Size::new(1000.0, 620.0))
        .run_with(|| (App::new(), Task::none()))
}
