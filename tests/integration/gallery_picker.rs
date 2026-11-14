use super::*;

const PUBLISH: &str = "/g/42/tickets/gallery/publish";

#[tokio::test]
async fn gallery_source_picker_shows_one_page_and_links_to_the_next() {
    let (config, state) = setup().await;
    state.lock().unwrap().gallery_sources = 30;
    let (status, html) = get_html(&config, PUBLISH).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("panel:src-000"));
    assert!(html.contains("panel:src-024"));
    assert!(
        !html.contains("panel:src-025"),
        "only the first page is listed"
    );
    assert!(
        html.contains("gallery-source-note"),
        "a partial list says so"
    );
    assert!(
        html.contains("gallery-source-search"),
        "the picker can be searched"
    );
    assert!(
        html.contains("cursor=src-025"),
        "the next page is linked with its cursor"
    );
    assert!(html.contains("gallery-source-more"));
}

#[tokio::test]
async fn gallery_source_cursor_shows_the_next_page_and_stops_at_the_last() {
    let (config, state) = setup().await;
    state.lock().unwrap().gallery_sources = 30;
    let (status, html) = get_html(&config, &format!("{PUBLISH}?cursor=src-025")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("panel:src-025"));
    assert!(html.contains("panel:src-029"));
    assert!(!html.contains("panel:src-024"));
    assert!(
        !html.contains("gallery-source-more"),
        "the last page links no further"
    );
}

#[tokio::test]
async fn gallery_source_search_narrows_the_list() {
    let (config, state) = setup().await;
    state.lock().unwrap().gallery_sources = 30;
    let (status, html) = get_html(&config, &format!("{PUBLISH}?q=Source%20027")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("panel:src-027"));
    assert!(!html.contains("panel:src-000"));
    assert!(
        html.contains("value=\"Source 027\""),
        "the search keeps its text"
    );
    let sent = state.lock().unwrap().queries.iter().any(|query| {
        query.contains("ticket-actions/gallery/gallery?") && query.contains("q=Source+027")
    });
    assert!(sent, "the search is sent to the plugin");
}

#[tokio::test]
async fn gallery_source_query_is_escaped_once_in_the_field_and_the_next_link() {
    let (config, state) = setup().await;
    state.lock().unwrap().gallery_sources = 30;
    let (status, html) = get_html(&config, &format!("{PUBLISH}?q=%26%20%3Cb%3E")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        html.contains(r#"value="&amp; &lt;b&gt;""#),
        "the field shows the text escaped once"
    );
    assert!(
        html.contains("q=%26+%3Cb%3E"),
        "the next link carries the query"
    );
    assert!(html.contains("cursor=src-025"));
    assert!(
        !html.contains("&amp;amp;"),
        "no double escape in the field or the link"
    );
    assert!(
        !html.contains("&amp;lt;"),
        "no double escape in the option labels"
    );
}

#[tokio::test]
async fn gallery_source_cursor_is_length_limited() {
    let (config, _) = setup().await;
    let long = "x".repeat(201);
    let (status, _) = get_html(&config, &format!("{PUBLISH}?cursor={long}")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
