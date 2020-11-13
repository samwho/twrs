use std::error::Error;
use std::result::Result;

use dialoguer::Input;

use egg_mode::{
    KeyPair,
    Token::{Access, Bearer},
};
use serde::{Deserialize, Serialize};

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let token = get_token().await?;

    let timeline = egg_mode::tweet::home_timeline(&token).with_page_size(10);

    let (timeline, feed) = timeline.start().await.unwrap();
    for tweet in feed.response {
        println!(
            "<@{}> {}",
            tweet.user.as_ref().unwrap().screen_name,
            tweet.text
        );
    }

    Ok(())
}

async fn get_token() -> Result<egg_mode::Token, Box<dyn Error>> {
    let home = match dirs::home_dir() {
        Some(d) => d,
        None => return Err("unable to find home directory".into()),
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
