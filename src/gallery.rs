use anyhow::Result;
use futures_util::StreamExt;
use indicatif::ProgressBar;
use reqwest::Url;
use std::{io::Write, path::PathBuf, sync::Arc, time::Duration};
use tokio::task::JoinSet;

use crate::{CLIENT, PB, SEM, config::Config, error, info};

#[derive(Debug)]
pub struct Gallery {
    pub url: Url,
    pub title: String,
    pub images: Vec<Url>,
}

impl Gallery {
    pub fn new(url: String) -> Result<Self> {
        Ok(Gallery {
            url: Url::parse(&url)?,
            title: String::new(),
            images: Vec::new(),
        })
    }

    pub async fn fetch_info(&mut self, config: Arc<Config>) -> Result<()> {
        let response = CLIENT
            .get()
            .unwrap()
            .get(self.url.as_str())
            .header("Cookie", &config.cookie)
            .send()
            .await?
            .text()
            .await?;

        let document = scraper::Html::parse_document(&response);
        let title = document
            .select(&scraper::Selector::parse("#gn").unwrap())
            .next()
            .map(|e| e.inner_html());

        if let Some(title) = title {
            self.title = title;
        } else {
            return Err(anyhow::anyhow!("Failed to find gallery title"));
        }

        Ok(())
    }

    pub async fn fetch_images(&mut self, config: Arc<Config>) -> Result<()> {
        let mut url = self.url.to_string();

        loop {
            let response = CLIENT
                .get()
                .unwrap()
                .get(url)
                .header("Cookie", &config.cookie)
                .send()
                .await?
                .text()
                .await?;

            let document = scraper::Html::parse_document(&response);
            let selector = scraper::Selector::parse("#gdt a").unwrap();

            for (index, element) in document.select(&selector).enumerate() {
                if let Some(src) = element.value().attr("href") {
                    if let Ok(image_url) = Url::parse(src) {
                        self.images.insert(index, image_url);
                    }
                }
            }

            // Check for next page link
            if let Some(next_page) = document
                .select(
                    &scraper::Selector::parse("table.ptt > tbody > tr > td:last-child > a")
                        .unwrap(),
                )
                .next()
            {
                if let Some(href) = next_page.value().attr("href") {
                    url = href.to_string();
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        Ok(())
    }

    pub async fn download(mut self, config: Arc<Config>) {
        if let Err(e) = self.fetch_info(Arc::clone(&config)).await {
            error!("Failed to fetch gallery info: {}", e);
            return;
        }

        if let Err(e) = self.fetch_images(Arc::clone(&config)).await {
            error!("Failed to fetch images: {}", e);
            return;
        }

        info!("Downloading gallery: {}", self.title);

        let mut tasks = JoinSet::new();
        let pb = Arc::new(PB.add(ProgressBar::new(self.images.len() as u64)));
        pb.enable_steady_tick(Duration::from_millis(100));
        pb.set_style(
            indicatif::ProgressStyle::default_bar()
                .template("[{wide_bar:cyan/blue}] [{pos}/{len}] [{msg}] ({elapsed_precise})")
                .unwrap()
                .progress_chars("=>-"),
        );

        let title = Arc::new(self.title);

        for (index, image_url) in self.images.into_iter().enumerate() {
            let title = Arc::clone(&title);
            let ext = image_url.as_str().rsplit('.').next().unwrap_or("jpg");
            let output_path = format!("{}/{}/{}.{}", config.output, title, index, ext);
            let pb = Arc::clone(&pb);
            tasks.spawn(async move {
                let _limit = SEM.get().unwrap().acquire().await;
                pb.set_message(format!("Downloading {} image {}", title, index + 1));
                download(output_path, image_url).await;
                pb.inc(1);
            });
        }

        tasks.join_all().await;

        pb.finish_and_clear();
    }
}

async fn download(output_path: String, url: Url) {
    let output_path = PathBuf::from(output_path);
    if !output_path.exists() {
        let _ = std::fs::create_dir_all(output_path.parent().unwrap()).map_err(|e| {
            error!("Failed to create output directory: {}", e);
            std::process::exit(1);
        });
    }

    let response = CLIENT
        .get()
        .unwrap()
        .get(url.as_str())
        .send()
        .await
        .expect("Failed to send request");

    if response.status().is_success() {
        let mut stream = response.bytes_stream();
        let mut file = std::fs::File::create(output_path).expect("Failed to create file");
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.expect("Failed to read chunk");
            file.write_all(&chunk).expect("Failed to write chunk");
        }
    } else {
        error!(
            "Failed to download image from {}, status: {}",
            url,
            response.status()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_gallery() {
        let url = "https://example.com/gallery".to_string();
        let gallery = Gallery::new(url).unwrap();
        assert_eq!(gallery.url.as_str(), "https://example.com/gallery");
    }
}
