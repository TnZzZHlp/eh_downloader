use std::sync::Arc;

use anyhow::Result;
use reqwest::Url;
use tokio::task::JoinSet;

use crate::{CLIENT, config::Config, error, info};

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

    pub async fn download_image(&mut self, config: Arc<Config>, index: usize) -> Result<()> {
        for image_url in &self.images {
            let response = CLIENT
                .get()
                .unwrap()
                .get(image_url.as_str())
                .header("Cookie", &config.cookie)
                .send()
                .await?
                .bytes()
                .await?;

            // Here you would implement the logic to save the image bytes to a file
            // For now, we just print the image URL
            info!("Downloading image: {}", image_url);
        }

        Ok(())
    }

    pub async fn download(&mut self, config: Arc<Config>) {
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

        self.images.iter().enumerate().for_each(|(index, _)| {
            let config = Arc::clone(&config);
            tasks.spawn(async move {
                if let Err(e) = self.download_image(config, index).await {
                    error!("Failed to download image {}: {}", index, e);
                }
            });
        });
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
