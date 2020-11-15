use chrono::{DateTime, Utc};
use std::time::Duration;
use std::{collections::BTreeMap, io};
use termion::{input::MouseTerminal, raw::IntoRawMode, screen::AlternateScreen};
use tui::{
    backend::TermionBackend,
    buffer::Buffer,
    layout::Rect,
    style::Modifier,
    style::{Color, Style},
    text::{Span, Spans},
    widgets::Widget,
    Terminal,
};

use dialoguer::Input;

use egg_mode::{
    tweet::{Timeline, Tweet},
    KeyPair,
    Token::{Access, Bearer},
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
enum Error {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("config loading error: {0}")]
    Config(&'static str),

    #[error("config loading error: {0}")]
    TOMLDeserialize(#[from] toml::de::Error),

    #[error("config saving error: {0}")]
    TOMLSerialize(#[from] toml::ser::Error),

    #[error("twitter error: {0}")]
    Twitter(#[from] egg_mode::error::Error),
}

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    twitter: Twitter,
}

#[derive(Debug, Serialize, Deserialize)]
struct Twitter {
    key: String,
    secret: String,
    token: Option<Token>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Token {
    consumer: KeyPair,
    access: KeyPair,
}

impl From<egg_mode::Token> for Token {
    fn from(t: egg_mode::Token) -> Self {
        match t {
            Access { consumer, access } => Token { access, consumer },
            Bearer(_) => panic!("wrong token type"),
        }
    }
}

impl From<Token> for egg_mode::Token {
    fn from(t: Token) -> Self {
        Access {
            consumer: t.consumer,
            access: t.access,
        }
    }
}

struct TimelineRenderer {
    timeline: Timeline,
    tweets: BTreeMap<DateTime<Utc>, Tweet>,
}

impl TimelineRenderer {
    fn new(timeline: Timeline) -> Self {
        TimelineRenderer {
            timeline,
            tweets: BTreeMap::new(),
        }
    }

    async fn update(mut self) -> Result<TimelineRenderer> {
        let (new_timeline, response) = self.timeline.newer(None).await?;
        self.timeline = new_timeline;
        for tweet in response.response {
            self.tweets.insert(tweet.created_at.clone(), tweet);
        }
        Ok(self)
    }
}

impl Widget for &TimelineRenderer {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let colors = colorous::TABLEAU10;

        let list_items: Vec<tui::widgets::ListItem> = self
            .tweets
            .iter()
            .rev()
            .enumerate()
            .map(|(i, (_, tweet))| {
                let c = colors[i % colors.len()];

                let sep = Span::from(" ");
                let timestamp = Span::styled(
                    tweet.created_at.format("%H:%M:%S").to_string(),
                    Style::default().fg(Color::DarkGray),
                );
                let username = Span::styled(
                    tweet.user.clone().unwrap().screen_name,
                    Style::default()
                        .fg(Color::Rgb(c.r, c.g, c.b))
                        .add_modifier(Modifier::BOLD),
                );
                let text = Span::styled(tweet.text.clone(), Style::default());

                let spans = Spans::from(vec![timestamp, sep.clone(), username, sep, text]);
                tui::widgets::ListItem::new(spans)
            })
            .collect();
        let list = tui::widgets::List::new(list_items);

        list.render(area, buf);
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let stdout = io::stdout().into_raw_mode()?;
    let stdout = MouseTerminal::from(stdout);
    let stdout = AlternateScreen::from(stdout);
    let backend = TermionBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let token = get_token().await?;
    let timeline = egg_mode::tweet::home_timeline(&token).with_page_size(30);

    let mut widget = TimelineRenderer::new(timeline);

    loop {
        widget = widget.update().await?;

        terminal.draw(|f| {
            f.render_widget(&widget, f.size());
        })?;

        std::thread::sleep(Duration::from_millis(5000));
    }
}

async fn get_token() -> Result<egg_mode::Token> {
    let home = match dirs::home_dir() {
        Some(d) => d,
        None => return Err(Error::Config("unable to find home directory")),
    };

    let config_path = home.join(".config").join("twrs").join("config.toml");
    let mut config: Config = toml::from_str(&std::fs::read_to_string(&config_path)?)?;

    let token: egg_mode::Token = match config.twitter.token.clone() {
        Some(t) => t.into(),
        None => {
            let con_token =
                egg_mode::KeyPair::new(config.twitter.key.clone(), config.twitter.secret.clone());
            let request_token = egg_mode::auth::request_token(&con_token, "oob").await?;
            let auth_url = egg_mode::auth::authorize_url(&request_token);

            println!("visit {}", auth_url);
            let pin: String = Input::new().with_prompt("PIN").interact_text()?;
            let (token, _, _) =
                egg_mode::auth::access_token(con_token, &request_token, pin).await?;
            config.twitter.token = Some(token.clone().into());
            std::fs::write(&config_path, toml::to_string_pretty(&config)?)?;
            token
        }
    };

    Ok(token)
}
