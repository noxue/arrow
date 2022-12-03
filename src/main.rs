use std::{
    fs::{read_to_string, File},
    path::Path,
    time::Duration,
};

use lettre::{smtp::authentication::Credentials, SmtpClient, Transport};
use lettre_email::EmailBuilder;
use regex::Regex;
use reqwest::{header::{HeaderMap, ACCEPT, REFERER, USER_AGENT, HeaderValue}, Proxy};
use serde::{Deserialize, Serialize};

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
                                format!("[arrow艾睿] {} 产品有 {} 个新库存", &product, count)
                                    .as_str(),
                                format!("[arrow艾睿] {} 产品有 {} 个新库存", &product, count)
                                    .as_str(),
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

/*
{"data":{"results":{"products":[{"npi":false,"promoGroup":"","text":"LM258DR","searchUrl":"/en/products/search?selectedType=product&q=LM258DR"},{"npi":false,"promoGroup":"","text":"LM258DR2G","searchUrl":"/en/products/search?selectedType=product&q=LM258DR2G"},{"npi":false,"promoGroup":"","text":"LM258DRG3","searchUrl":"/en/products/search?selectedType=product&q=LM258DRG3"},{"npi":false,"promoGroup":"","text":"LM258DRG4","searchUrl":"/en/products/search?selectedType=product&q=LM258DRG4"},{"npi":false,"promoGroup":"","text":"LM258DR2","searchUrl":"/en/products/search?selectedType=product&q=LM258DR2"}],"productLines":[],"descriptions":[],"manufacturers":[],"suggestions":[],"referenceDesigns":[]}},"error":null}
*/
#[derive(Serialize, Deserialize, Debug)]
struct Product {
    npi: bool,
    promo_group: String,
    text: String,
    search_url: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct Result {
    products: Vec<Product>,
    product_lines: Vec<String>,
    descriptions: Vec<String>,
    manufacturers: Vec<String>,
    suggestions: Vec<String>,
    reference_designs: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Data {
    results: Result,
}

#[derive(Serialize, Deserialize, Debug)]
struct Response {
    data: Data,
    error: String,
}

async fn get_stores(product: &str) -> Option<i32> {
    // 根据获取到的cookie创建 reqwest client
    let client = {
        let default_headers = gen_default_headers();

        reqwest::Client::builder()
            .default_headers(default_headers.clone())
            // .cookie_store(true)
            // 代理 127.0.0.1:8888
            .proxy(
                Proxy::http("http://localhost:8888").unwrap()
            )
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap()
    };

    let res = match client
        .get(format!(
            "https://www.arrow.com/apps/search/api/autocomplete?q={}&lang=en",
            product
        ))
        .send()
        .await
    {
        Ok(v) => v,
        Err(e) => {
            println!("请求出错:{}", e);
            return None;
        }
    };

    let res: Response = match res.json().await {
        Ok(v) => v,
        Err(e) => {
            println!("解析出错:{}", e);
            return None;
        }
    };

    println!("获取到的产品列表:{:#?}", res);

    // let html = match res.text().await {
    //     Ok(v) => v,
    //     Err(e) => {
    //         log::error!("获取网页代码出错:{}", e);
    //         return None;
    //     }
    // };

    // // 正则匹配库存数, 4,970 parts
    // let re = Regex::new(r#"([,\d]+) parts"#).unwrap();

    // let mut caps = re.captures_iter(&html);
    // let v = match caps.next() {
    //     Some(v) => v,
    //     None => {
    //         return None;
    //     }
    // };

    // let store: i32 = v[1].replace(",", "").parse().unwrap();
    // log::debug!("{:#?}", store);

    // Some(store)
    None
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
    
    default_headers.insert("accept", "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.9".parse().unwrap());
    default_headers.insert("accept-encoding", "gzip, deflate, br".parse().unwrap());
    default_headers.insert("accept-language", "zh".parse().unwrap());
    default_headers.insert("cache-control", "no-cache".parse().unwrap());
    default_headers.insert("pragma", "no-cache".parse().unwrap());
    default_headers.insert("sec-ch-ua", "\"Google Chrome\";v=\"107\", \"Chromium\";v=\"107\", \"Not=A?Brand\";v=\"24\"".parse().unwrap());
    default_headers.insert("sec-ch-ua-mobile", "?0".parse().unwrap());
    default_headers.insert("sec-ch-ua-platform", "\"Windows\"".parse().unwrap());
    default_headers.insert("sec-fetch-dest", "document".parse().unwrap());
    default_headers.insert("sec-fetch-mode", "navigate".parse().unwrap());
    default_headers.insert("sec-fetch-site", "none".parse().unwrap());
    default_headers.insert("sec-fetch-user", "?1".parse().unwrap());
    default_headers.insert("upgrade-insecure-requests", "1".parse().unwrap());
    default_headers.insert("user-agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/107.0.0.0 Safari/537.36".parse().unwrap());

    default_headers
}


#[test]
fn test_get_stores() {
    let  rt = tokio::runtime::Runtime::new().unwrap();
    let res = rt.block_on(async{
        get_stores("LM258DR").await;
    });
    println!("{:#?}", res);
}

