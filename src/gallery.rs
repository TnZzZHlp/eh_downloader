use anyhow::Result;
use futures_util::StreamExt;
use indicatif::ProgressBar;
use reqwest::Url;
use std::{io::Write, path::PathBuf, sync::Arc, time::Duration};
use tokio::task::JoinSet;

use crate::{CLIENT, PB, SEM, config::Config, error};

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

        let mut tasks = JoinSet::new();
        let pb = Arc::new(PB.add(ProgressBar::new(self.images.len() as u64)));
        pb.enable_steady_tick(Duration::from_millis(100));
        pb.set_style(
            indicatif::ProgressStyle::default_bar()
                .template("[{wide_bar:.cyan/blue}] [{pos}/{len}] [{msg:<7}] ({elapsed_precise})")
                .unwrap()
                .progress_chars("=>-"),
        );

        let title = Arc::new(
            self.title
                .replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], ""),
        );

        for (index, image_url) in self.images.into_iter().enumerate() {
            let title = Arc::clone(&title);
            let config = Arc::clone(&config);
            let pb = Arc::clone(&pb);
            tasks.spawn(async move {
                let _limit = SEM.get().unwrap().acquire().await;
                pb.set_message(format!("Downloading image {}", index + 1));
                download(index, title, image_url, config).await;
                pb.inc(1);
            });
        }

        tasks.join_all().await;

        pb.finish_and_clear();
    }
}

async fn download(index: usize, title: Arc<String>, url: Url, config: Arc<Config>) {
    let response = CLIENT
        .get()
        .unwrap()
        .get(url.as_str())
        .header("Cookie", &config.cookie)
        .send()
        .await
        .expect("Failed to send request");

    if !response.status().is_success() {
        error!(
            "Failed to download image from {}, status: {}",
            url,
            response.status()
        );
    }

    let mut image_url = String::new();
    if config.original {
        {
            let document = scraper::Html::parse_document(&response.text().await.unwrap());
            let selector = scraper::Selector::parse("div#i6 div:last-child a").unwrap();
            if let Some(element) = document.select(&selector).next() {
                if let Some(href) = element.value().attr("href") {
                    image_url = href.to_string();
                }
            }
        }

        let redirect_url = CLIENT
            .get()
            .unwrap()
            .get(image_url.as_str())
            .header("Cookie", &config.cookie)
            .send()
            .await
            .expect("Failed to send request for image redirect");

        if redirect_url.status().is_redirection() {
            if let Some(location) = redirect_url.headers().get(reqwest::header::LOCATION) {
                if let Ok(loc_str) = location.to_str() {
                    image_url = loc_str.to_string();
                }
            }
        }
    } else {
        let selector = scraper::Selector::parse("div#i3 a img").unwrap();
        let document = scraper::Html::parse_document(&response.text().await.unwrap());
        if let Some(element) = document.select(&selector).next() {
            if let Some(src) = element.value().attr("src") {
                image_url = src.to_owned();
            }
        }
    }

    if image_url.is_empty() {
        error!("No image found for index {} at {}", index, url);
        return;
    }

    let response = CLIENT
        .get()
        .unwrap()
        .get(&image_url)
        .send()
        .await
        .expect("Failed to send request for image");

    if !response.status().is_success() {
        error!(
            "Failed to download image from {}, status: {}",
            image_url,
            response.status()
        );
        return;
    }

    let ext = image_url.rsplit('.').next().unwrap_or("jpg");
    let output_dir = PathBuf::from(&format!("{}/{}", config.output, title));
    if !output_dir.exists() {
        std::fs::create_dir_all(&output_dir).expect("Failed to create output directory");
    }

    let file_path = output_dir.join(format!("{}.{}", index + 1, ext));
    let mut file = std::fs::File::create(&file_path).expect("Failed to create file");

    let mut stream = response.bytes_stream();

    while let Some(bytes) = stream.next().await {
        file.write_all(&bytes.unwrap())
            .expect("Failed to write chunk to file");
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
