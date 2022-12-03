use std::{
    fs::{read_to_string, File},
    path::Path,
    time::Duration,
};

use lettre::{smtp::authentication::Credentials, SmtpClient, Transport};
use lettre_email::EmailBuilder;
use regex::Regex;
use reqwest::header::{HeaderMap, ACCEPT, REFERER, USER_AGENT};

#[tokio::main]
async fn main() {
    log4rs::init_file("log4rs.yaml", Default::default()).unwrap();

    // 接收到列表才执行
    let products;

    // 产品列表
    if Path::new("./products.txt").exists() {
        let data = read_to_string("./products.txt").unwrap_or("".to_owned());
        products = data
            .split("\n")
            .into_iter()
            .map(|v| v.trim().to_string())
            .collect::<Vec<String>>();
    } else {
        File::create("./products.txt").unwrap();
        println!("请配置产品");
        return;
    }

    // 邮箱信息
    let mut email_from = "".to_owned();
    let mut email_from_password = "".to_owned();
    let mut emails_to = vec![];
    // 邮箱配置信息
    if Path::new("./email.txt").exists() {
        let data = read_to_string("./email.txt").unwrap_or("".to_owned());
        let t = data.split("\n").map(|v| v).collect::<Vec<&str>>();
        if t.len() >= 3 {
            let mut it = t.iter();
            email_from = it.next().unwrap().trim().to_string().clone();
            email_from_password = it.next().unwrap().trim().to_string().clone();

            for v in it {
                if v.trim().is_empty() {
                    continue;
                }
                emails_to.push(v.trim().to_string().clone());
            }
        }
    } else {
        File::create("./email.txt").unwrap();
        println!("请配置邮件信息");
        return;
    }

    log::debug!("要监控的产品列表:{:#?}", products);

    let mut tasks = vec![];
    loop {
        for product in &products {
            let product = product.clone();
            let email_from = email_from.clone();
            let email_from_password = email_from_password.clone();
            let emails_to = emails_to.clone();

            let v = tokio::spawn(async move {
                println!("开始检测 [{}] 的库存", &product);
                // 如果有库存，就发邮件
                if let Some(count) = get_stores(&product).await {
                    if count > 0 {
                        log::info!("发送邮件,库存:{}", count);

                        // 给所有收件箱发送邮件
                        for email_to in emails_to {
                            println!(
                                "发送邮件给{}=>{}",
                                &email_to,
                                format!("{} 产品有 {} 个新库存", &product, count)
                            );
                            send_email(
                                &email_from,
                                &email_from_password,
                                &email_to,
                                format!("[arrow艾睿] {} 产品有 {} 个新库存", &product, count).as_str(),
                                format!("[arrow艾睿] {} 产品有 {} 个新库存", &product, count).as_str(),
                            );
                        }
                    } else {
                        println!("[{}] 的无库存", &product);
                    }
                }
            });
            tasks.push(v);

            // 限制并发数
            while tasks.len() > 50 {
                if let Some(task) = tasks.pop() {
                    task.await.unwrap();
                }
            }
        }
    }
}

async fn get_stores(product: &str) -> Option<i32> {
    // 根据获取到的cookie创建 reqwest client
    let client = {
        let default_headers = gen_default_headers();

        reqwest::Client::builder()
            .default_headers(default_headers.clone())
            .cookie_store(true)
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap()
    };

    let res = match client
        .get(format!(
            "https://www.arrow.com/en/products/search?cat=&q={}&r=true",
            product
        ))
        .send()
        .await
    {
        Ok(v) => v,
        Err(e) => {
            log::error!("请求出错:{}", e);
            return None;
        }
    };

    let html = match res.text().await {
        Ok(v) => v,
        Err(e) => {
            log::error!("获取网页代码出错:{}", e);
            return None;
        }
    };

    // 正则匹配库存数, 4,970 parts
    let re = Regex::new(r#"([,\d]+) parts"#).unwrap();

    let mut caps = re.captures_iter(&html);
    let v = match caps.next() {
        Some(v) => v,
        None => {
            return None;
        }
    };

    let store: i32 = v[1].replace(",", "").parse().unwrap();
    log::debug!("{:#?}", store);

    Some(store)
}

fn send_email(from: &str, password: &str, to: &str, title: &str, body: &str) {
    log::debug!("发件箱:{}, 收件箱:{}", from, to);
    let email = EmailBuilder::new()
        .from(from)
        .to(to)
        .subject(title)
        .html(body)
        .build()
        .unwrap();

    let mut mailer = SmtpClient::new_simple("smtp.qq.com")
        .unwrap()
        .credentials(Credentials::new(from.into(), password.into()))
        .transport();

    match mailer.send(email.into()) {
        Ok(v) => v,
        Err(e) => {
            log::error!("发送邮件出错：{}", e);
            println!("发送邮件出错,{}", e);
            return;
        }
    };
}

fn gen_default_headers() -> HeaderMap {
    let mut default_headers = HeaderMap::new();

    default_headers.insert(USER_AGENT, "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/96.0.4664.110 Safari/537.36".parse().unwrap());
    default_headers.insert(REFERER, "https://www.arrow.com/en/npi".parse().unwrap());
    default_headers.insert(
        "sec-ch-ua",
        r#"" Not A;Brand";v="99", "Chromium";v="96", "Google Chrome";v="96""#
            .parse()
            .unwrap(),
    );
    default_headers.insert("sec-ch-ua-mobile", r#"?0"#.parse().unwrap());
    default_headers.insert("sec-ch-ua-platform", r#""Windows""#.parse().unwrap());
    default_headers.insert("Upgrade-Insecure-Requests", r#"1"#.parse().unwrap());
    default_headers.insert(ACCEPT,r#"text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.9"#.parse().unwrap());

    default_headers.insert("Sec-Fetch-Site", r#"same-origin"#.parse().unwrap());
    default_headers.insert("Sec-Fetch-Mode", r#"navigate"#.parse().unwrap());
    default_headers.insert("Sec-Fetch-User", r#"?1"#.parse().unwrap());
    default_headers.insert("Sec-Fetch-Dest", r#"document"#.parse().unwrap());
    default_headers.insert("Accept-Language", r#"zh"#.parse().unwrap());
    default_headers.insert("Accept-Encoding", r#"gzip, deflate, br"#.parse().unwrap());

    default_headers
}
