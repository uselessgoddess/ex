use {
    figment::{
        providers::{Format, Toml},
        Figment,
    },
    futures_util::{StreamExt, TryStreamExt},
    indicatif::{ProgressBar, ProgressStyle},
    regex::Regex,
    reqwest::{Client, Response},
    serde::Deserialize,
    std::{
        env,
        error::Error,
        fs::{self, File},
        io::{Read, Write},
    },
};

type Result<T, E = Box<dyn Error>> = std::result::Result<T, E>;

#[derive(Deserialize)]
struct Config {
    domain: Option<String>,
    url: Option<String>,
    re: Option<String>,
}

async fn prepare_download(client: &Client, url: &str) -> Result<(Response, u64)> {
    let req = client.get(url).send().await?;
    let len = req.content_length().ok_or(format!("failed to get content length from `{url}`"))?;
    Ok((req, len))
}

async fn download_bytes(client: &Client, url: &str) -> Result<Vec<u8>> {
    let (req, total_length) = prepare_download(client, url).await?;
    let pb = ProgressBar::new(total_length).with_style(
        ProgressStyle::default_bar()
            .template(
                "{msg}\n{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] \
                 {bytes}/{total_bytes} ({bytes_per_sec}, {eta})",
            )?
            .progress_chars("#>-"),
    );

    let mut buf = Vec::with_capacity(total_length as usize);
    let mut stream = req.bytes_stream();
    while let Some(chunk) = stream.try_next().await? {
        pb.inc(chunk.len() as u64);
        buf.extend(chunk);
    }

    Ok(buf)
}

#[tokio::main]
async fn main() -> Result<()> {
    let home = home::home_dir().unwrap().join(".ex");
    let _ = fs::create_dir(&home);

    let Config { domain, url, re } =
        Figment::new().merge(Toml::file(home.join("conf.toml"))).extract()?;

    let mut client = Client::new();
    let domain = domain.as_deref().unwrap_or("http://mininform.gov.by");
    let url =
        url.as_deref().unwrap_or("/documents/respublikanskiy-spisok-ekstremistskikh-materialov/");
    let url = format!("{domain}{url}");
    let re = Regex::new(re.as_deref().unwrap_or(r#"<a download href="([A-Za-z0-9./]*)">"#))?;

    let html = client.get(url).send().await?.text().await?;
    let doc = format!("{domain}{}", re.captures(&html).unwrap().get(1).unwrap().as_str());

    let (_, len) = prepare_download(&client, &doc).await?;

    let mut cache = File::options().read(true).write(true).open(home.join("cache"))?;

    let bytes = if cache.metadata()?.len() != len {
        let bytes = download_bytes(&client, &doc).await?;
        cache.write_all(&bytes)?;
        bytes
    } else {
        let mut buf = Vec::with_capacity(len as usize);
        cache.read_to_end(&mut buf)?;
        buf
    };

    Ok(())
}
