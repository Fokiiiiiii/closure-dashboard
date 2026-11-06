use super::*;

pub(super) fn bot_data(
    fake: &mut Fake,
    path: &str,
    method: &Method,
    body: &[u8],
) -> (StatusCode, [(header::HeaderName, &'static str); 1], String) {
    let fixture = |source: &str| serde_json::from_str::<serde_json::Value>(source).unwrap();
    let data = if path == "/v1/schema" {
        let mut schema = fixture(include_str!("../fixtures/schema-basic.json"));
        if fake.structured {
            schema["properties"]["modules"]["anyOf"][0]["properties"]["welcome"]["properties"]["config"]
                ["anyOf"][0]["properties"]["message"] = json!({
                "anyOf": [{"type": "string", "x-ui": {"widget": "textarea"}}, {"type": "object"}, {"type": "null"}],
                "x-ui": {"widget": "message", "variables": ["user", "server"]}
            });
            let mut parts = fixture(include_str!("../fixtures/automod-structured.json"));
            for (name, widget) in [
                ("rules", "entries-json"),
                ("counters", "entries-json"),
                ("antiraid", "json"),
            ] {
                parts[name]["x-ui"] = json!({"level": "basic", "widget": widget});
            }
            parts["dry_run"] = json!({"type": "boolean"});
            schema["properties"]["modules"]["anyOf"][0]["properties"]["automod"]["properties"]["config"] =
                json!({"anyOf": [{"type": "object", "properties": parts}]});
        }
        if fake.rules {
            schema["properties"]["modules"]["anyOf"][0]["properties"]["channels"]["properties"]["config"] = json!({"anyOf":[{"type":"object","properties":{"auto_delete":{
                "type":"array","maxItems":25,"items":{"type":"object","required":["channel","after"],
                "properties":{
                    "channel":{"type":"string","pattern":"^[1-9][0-9]{0,19}$"},
                    "after":{"type":"string","pattern":"^[0-9]+[smhdw]( *[0-9]+[smhdw])*$"},
                    "skip_pinned":{"type":"boolean","default":true}}}},
                "texts":{"description":"texts","x-ui":{"widget":"entries"}},
                "keep":{"x-ui":{"widget":"flag-or-roles"}}}}]});
        }
        schema["properties"]["timezone"]["anyOf"][0]["description"] =
            json!("<script>description</script>");
        schema["properties"]["modules"]["anyOf"][0]["properties"]["welcome"]["properties"]["config"]
            ["anyOf"][0]["properties"]["choice"] =
            json!({"type":"string","enum":["one","<script>"]});
        schema["properties"]["modules"]["anyOf"][0]["properties"]["welcome"]["properties"]["config"]
            ["anyOf"][0]["properties"]["role"] = json!({"type":"string","x-ui":{"widget":"role"}});
        schema["properties"]["modules"]["anyOf"][0]["properties"]["welcome"]["properties"]["config"]
            ["anyOf"][0]["properties"]["<script>"] =
            json!({"type":"string","description":"<script>description</script>"});
        schema
    } else if path.ends_with("/messages") && *method == Method::POST {
        fake.form_body = Some(serde_json::from_slice(body).unwrap());
        json!({"id": "77", "channel_id": "11", "url": "https://discord.com/channels/42/11/77"})
    } else if path.ends_with("/config") && *method == Method::PUT {
        fake.form_body = Some(serde_json::from_slice(body).unwrap());
        if let Some(status) = fake.form_put_status {
            let path = if fake
                .form_body
                .as_ref()
                .is_some_and(|b| b.get("timezone").is_some() && b.get("modules").is_none())
            {
                "timezone"
            } else {
                "modules.welcome.config.channel"
            };
            return (status, [(header::ETAG, "\"1\"")], json!({"error":{"message":"bad","issues":[{"path":path,"message":"<script>bad</script>"}]}}).to_string());
        }
        json!({"revision":2})
    } else if path.ends_with("/config") {
        let mut config = fixture(include_str!("../fixtures/config-basic.json"));
        config["moderator_roles"] = json!([13, "99"]);
        config["modules"]["moderation"]["config"]["timeout_role"] = json!("13");
        config
    } else if path.ends_with("/effective") {
        fixture(include_str!("../fixtures/api-eff.json"))
    } else if path.ends_with("/directory") {
        if fake.directory_failure {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                [(header::ETAG, "\"1\"")],
                "{}".into(),
            );
        }
        json!({"channels":[{"id":"11","name":"<script>","kind":"text"},{"id":"12","name":"voice","kind":"voice"}],"roles":[{"id":"42","name":"@everyone","managed":false},{"id":"13","name":"<script>role</script>","managed":false},{"id":"14","name":"managed","managed":true}]})
    } else if path.ends_with("/access/123") {
        json!({"allowed":true})
    } else if path == "/v1/guilds" {
        json!([{"id":"42","name":"Server"}])
    } else {
        json!({})
    };
    (StatusCode::OK, [(header::ETAG, "\"1\"")], data.to_string())
}

#[tokio::test]
async fn the_parts_of_a_module_are_pages_of_their_own() {
    let (config, state) = setup().await;
    {
        let mut state = state.lock().unwrap();
        state.forms = true;
        state.rules = true;
    }
    let main = "/g/42/modules/channels";
    let rules = format!("{main}/auto_delete");
    let texts = format!("{main}/texts");
    let page = html(response(&config, Method::GET, main, "").await).await;
    for expected in [
        "name=\"f.keep\"",
        "<nav class=\"tabs\">",
        "href=\"/g/42/modules/channels/auto_delete\"",
        "href=\"/g/42/modules/channels/texts\"",
    ] {
        assert!(page.contains(expected), "{expected}");
    }
    assert!(!page.contains("f.auto_delete") && !page.contains("f.texts"));
    let missing = response(&config, Method::GET, &format!("{main}/nothing"), "").await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);

    let page = html(response(&config, Method::GET, &rules, "").await).await;
    for expected in [
        "name=\"f.auto_delete.0.channel\"",
        "name=\"f.auto_delete.0.after\"",
        "name=\"f.auto_delete.0.skip_pinned\"",
        "<option value=\"11\">&lt;script&gt;</option>",
        "list-caption",
    ] {
        assert!(page.contains(expected), "{expected}");
    }
    assert!(!page.contains("f.auto_delete.0.x") && !page.contains("style="));
    let rule = "etag=%221%22&f.auto_delete.0.channel=11&f.auto_delete.0.after=1h+30m\
        &f.auto_delete.0.skip_pinned=false&f.auto_delete.0.skip_pinned=true";
    let result = response(&config, Method::POST, &rules, rule).await;
    assert_eq!(result.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        result.headers()[header::LOCATION],
        "/g/42/modules/channels/auto_delete?saved=1"
    );
    let sent = state.lock().unwrap().form_body.clone().unwrap();
    assert_eq!(
        sent["modules"]["channels"]["config"]["auto_delete"],
        json!([{"channel": "11", "after": "1h 30m", "skip_pinned": true}])
    );
    let result = response(&config, Method::POST, main, "etag=%221%22&f.keep=1%2C+22").await;
    assert_eq!(result.status(), StatusCode::SEE_OTHER);
    let sent = state.lock().unwrap().form_body.clone().unwrap();
    assert_eq!(
        sent["modules"]["channels"]["config"]["keep"],
        json!(["1", "22"])
    );
    let page = html(response(&config, Method::GET, &texts, "").await).await;
    assert!(page.contains("name=\"f.texts.0.k\""));
    let entries = "etag=%221%22&f.texts.0.k=sdk-done&f.texts.0.v=Noted.";
    let result = response(&config, Method::POST, &texts, entries).await;
    assert_eq!(result.status(), StatusCode::SEE_OTHER);
    let sent = state.lock().unwrap().form_body.clone().unwrap();
    assert_eq!(
        sent["modules"]["channels"]["config"]["texts"],
        json!({"sdk-done": "Noted."})
    );
    let result = response(
        &config,
        Method::POST,
        &rules,
        "etag=%221%22&f.auto_delete.0.channel=11&f.auto_delete.0.after=",
    )
    .await;
    assert_eq!(result.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let page = html(result).await;
    assert!(page.contains("role=\"alert\""));
    assert!(page.contains("<option value=\"11\" selected>"));
}

#[tokio::test]
async fn rules_are_cards_and_not_text() {
    let (config, state) = setup().await;
    {
        let mut state = state.lock().unwrap();
        state.forms = true;
        state.structured = true;
    }
    let rules = "/g/42/modules/automod/rules";
    let page = html(response(&config, Method::GET, rules, "").await).await;
    assert!(
        page.contains("ルールを追加") && page.contains("href=\"/g/42/modules/automod/rules/new\"")
    );
    assert!(page.contains("class=\"tabs\"") && page.contains("/g/42/modules/automod/antiraid"));
    let page = html(response(&config, Method::GET, &format!("{rules}/new"), "").await).await;
    for expected in [
        "name=\"__id\"",
        "name=\"r.name\"",
        "name=\"new.r.triggers\"",
        "name=\"new.r.actions\"",
        "禁止単語を含む",
        "メッセージを削除する",
    ] {
        assert!(page.contains(expected), "{expected}");
    }
    assert!(!page.contains("<textarea") && !page.contains("style="));

    // Adding a condition shows its fields and keeps what was typed.
    let add = "etag=%221%22&op=add%3Ar.triggers&new.r.triggers=words&__id=spam&r.name=Spam";
    let page = html(response(&config, Method::POST, &format!("{rules}/new"), add).await).await;
    assert!(page.contains("name=\"r.triggers.0.words\"") && page.contains("value=\"spam\""));

    let save = "etag=%221%22&op=save&__id=spam&r.name=Spam&r.triggers.0.__t=words&r.triggers.0.words=bad%0Aworse\
        &r.actions.0.__t=reply&r.actions.0.text=No&r.actions.1.__t=delete&r.enabled=true";
    let result = response(&config, Method::POST, &format!("{rules}/new"), save).await;
    assert_eq!(result.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        result.headers()[header::LOCATION],
        "/g/42/modules/automod/rules/spam?saved=1"
    );
    let sent = state.lock().unwrap().form_body.clone().unwrap();
    assert_eq!(
        sent["modules"]["automod"]["config"]["rules"]["spam"],
        json!({
            "name": "Spam",
            "triggers": [{"type": "words", "words": ["bad", "worse"]}],
            "actions": [{"do": "reply", "text": "No"}, {"do": "delete"}]
        })
    );

    // A rule without an action, and a name that is not allowed, are refused.
    let bad = "etag=%221%22&op=save&__id=Bad+Name&r.triggers.0.__t=any_message";
    let result = response(&config, Method::POST, &format!("{rules}/new"), bad).await;
    assert_eq!(result.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let page = html(result).await;
    assert!(page.matches("role=\"alert\"").count() >= 2, "{page}");
    let result = response(
        &config,
        Method::POST,
        &format!("{rules}/new"),
        "etag=%221%22&op=nothing",
    )
    .await;
    assert_eq!(result.status(), StatusCode::BAD_REQUEST);
    let missing = response(
        &config,
        Method::GET,
        "/g/42/modules/automod/rules/nothing",
        "",
    )
    .await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_anti_raid_and_each_feature_have_a_switch_of_their_own() {
    let (config, state) = setup().await;
    {
        let mut state = state.lock().unwrap();
        state.forms = true;
        state.structured = true;
    }
    let path = "/g/42/modules/automod/antiraid";
    let page = html(response(&config, Method::GET, path, "").await).await;
    assert!(page.contains("name=\"r.levels\"") && !page.contains("name=\"on\""));
    assert!(page.contains("action=\"/g/42/features/automod/antiraid\""));
    let save = "etag=%221%22&op=save&r.levels=none%0Alow&r.enabled=false&r.effects.0.__k=low&r.effects.0.slowmode.__on=1&r.effects.0.slowmode.seconds=10";
    let result = response(&config, Method::POST, path, save).await;
    assert_eq!(result.status(), StatusCode::SEE_OTHER);
    let sent = state.lock().unwrap().form_body.clone().unwrap();
    assert_eq!(
        sent["modules"]["automod"]["config"]["antiraid"],
        json!({"levels": ["none", "low"], "enabled": false, "effects": {"low": {"slowmode": {"seconds": 10}}}})
    );

    // The tiles of the list switch the features, and the switch writes the feature's own setting.
    let list = html(response(&config, Method::GET, "/g/42/modules", "").await).await;
    for feature in ["rules", "antiraid"] {
        assert!(
            list.contains(&format!("action=\"/g/42/features/automod/{feature}\"")),
            "{feature}"
        );
    }
    let off = response(
        &config,
        Method::POST,
        "/g/42/features/automod/antiraid",
        "enabled=false",
    )
    .await;
    assert_eq!(off.status(), StatusCode::SEE_OTHER);
    let sent = state.lock().unwrap().form_body.clone().unwrap();
    assert_eq!(
        sent["modules"]["automod"]["config"]["antiraid"]["enabled"],
        json!(false)
    );
    let rules = response(
        &config,
        Method::POST,
        "/g/42/features/automod/rules",
        "enabled=false",
    )
    .await;
    assert_eq!(rules.status(), StatusCode::SEE_OTHER);
    let sent = state.lock().unwrap().form_body.clone().unwrap();
    assert_eq!(
        sent["modules"]["automod"]["config"]["rules_enabled"],
        json!(false)
    );
    for bad in ["enabled=maybe", ""] {
        let result = response(&config, Method::POST, "/g/42/features/automod/rules", bad).await;
        assert_eq!(result.status(), StatusCode::BAD_REQUEST, "{bad}");
    }
    let unknown = response(
        &config,
        Method::POST,
        "/g/42/features/automod/counters",
        "enabled=false",
    )
    .await;
    assert_eq!(unknown.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_message_is_written_previewed_and_sent_from_the_composer() {
    let (config, state) = setup().await;
    state.lock().unwrap().forms = true;
    let page = html(response(&config, Method::GET, "/g/42/compose", "").await).await;
    for expected in [
        "name=\"c\"",
        "name=\"channel\"",
        "<option value=\"11\"># &lt;script&gt;</option>",
        "id=\"preview\"",
        "data-preview=\"/g/42/compose?preview=1\"",
        "href=\"/g/42/compose\"",
        "src=\"/app.js\"",
    ] {
        assert!(page.contains(expected), "{expected}");
    }
    assert!(!page.contains("style=") && !page.contains("<script>"));

    let page =
        html(response(&config, Method::POST, "/g/42/compose", "op=add_embed&c=hi").await).await;
    assert!(page.contains("name=\"e.0.title\"") && page.contains("name=\"e.0.color\""));

    // The preview shows the message as Discord does, and never what the writer could use to attack.
    let draft = "c=**bold**+%3Cscript%3E&e.0.__=1&e.0.title=%3Cb%3E&e.0.url=javascript%3Aalert(1)&e.0.description=hello+https%3A%2F%2Fa.example\
        &e.0.color=%235865f2&e.0.f.0.__=1&e.0.f.0.name=n&e.0.f.0.value=v&e.0.f.0.inline=1&e.0.image=http%3A%2F%2Fi.example%2Fx.png";
    let fragment =
        html(response(&config, Method::POST, "/g/42/compose?preview=1", draft).await).await;
    for expected in [
        "dc-message",
        "dc-embed",
        "<strong>bold</strong>",
        "&lt;script&gt;",
        "fill=\"#5865f2\"",
        "dc-field inline",
        "href=\"https://a.example\"",
        "src=\"http://i.example/x.png\"",
    ] {
        assert!(fragment.contains(expected), "{expected}");
    }
    assert!(
        !fragment.contains("<script")
            && !fragment.contains("javascript:")
            && !fragment.contains("<b>")
    );

    // Sending needs a channel and something to say, and passes the message to the bot as it was written.
    let empty =
        html(response(&config, Method::POST, "/g/42/compose", "op=send&c=hello").await).await;
    assert!(empty.contains("role=\"alert\""));
    let nothing =
        html(response(&config, Method::POST, "/g/42/compose", "op=send&channel=11").await).await;
    assert!(nothing.contains("role=\"alert\""));
    assert!(state.lock().unwrap().form_body.is_none());
    let sent = html(
        response(
            &config,
            Method::POST,
            "/g/42/compose",
            &format!("op=send&channel=11&{draft}"),
        )
        .await,
    )
    .await;
    assert!(
        sent.contains("role=\"status\"") && sent.contains("https://discord.com/channels/42/11/77")
    );
    let body = state.lock().unwrap().form_body.clone().unwrap();
    assert_eq!(body["channel_id"], "11");
    assert_eq!(body["message"]["embeds"][0]["color"], "#5865f2");
    assert_eq!(
        body["message"]["embeds"][0]["fields"],
        json!([{"name": "n", "value": "v", "inline": true}])
    );
    assert_eq!(
        response(&config, Method::POST, "/g/42/compose", "op=nothing")
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    let script = response(&config, Method::GET, "/app.js", "").await;
    assert_eq!(script.status(), StatusCode::OK);
    assert!(
        script.headers()[header::CONTENT_SECURITY_POLICY]
            .to_str()
            .unwrap()
            .contains("script-src 'self'")
    );
}

#[tokio::test]
async fn a_post_can_carry_role_buttons_replace_a_message_and_wait_for_its_time() {
    let (config, state) = setup().await;
    state.lock().unwrap().forms = true;
    let page = html(response(&config, Method::GET, "/g/42/compose", "").await).await;
    for expected in [
        "name=\"edit\"",
        "name=\"when\"",
        "name=\"tz\"",
        "value=\"schedule\"",
        "value=\"add_button\"",
    ] {
        assert!(page.contains(expected), "{expected}");
    }
    let page =
        html(response(&config, Method::POST, "/g/42/compose", "op=add_button&c=hi").await).await;
    assert!(page.contains("name=\"b.0.label\"") && page.contains("name=\"b.0.role\""));

    let fragment = html(
        response(
            &config,
            Method::POST,
            "/g/42/compose?preview=1",
            "c=hi&b.0.__=1&b.0.label=%3Cb%3ENews&b.0.role=9",
        )
        .await,
    )
    .await;
    assert!(fragment.contains("dc-button") && fragment.contains("&lt;b&gt;News"));

    // 2026-11-06 09:30 in Tokyo.
    let scheduled = html(
        response(
            &config,
            Method::POST,
            "/g/42/compose",
            "op=schedule&channel=11&tz=-540&when=2026-11-06T09%3A30&c=hi&b.0.__=1&b.0.label=News&b.0.role=9",
        )
        .await,
    )
    .await;
    assert!(scheduled.contains("role=\"status\"") && scheduled.contains("2026-11-06 09:30"));
    let body = state.lock().unwrap().form_body.clone().unwrap();
    assert_eq!(body["send_at"], 1_793_925_000);
    assert_eq!(
        body["message"]["buttons"],
        json!([{"label": "News", "role": "9"}])
    );
    assert!(body.get("edit").is_none());

    // A link to a message names the message and its channel.
    let edited = html(
        response(
            &config,
            Method::POST,
            "/g/42/compose",
            "op=send&channel=11&edit=https%3A%2F%2Fdiscord.com%2Fchannels%2F42%2F12%2F345&c=new",
        )
        .await,
    )
    .await;
    assert!(edited.contains("role=\"status\""));
    let body = state.lock().unwrap().form_body.clone().unwrap();
    assert_eq!(
        (body["edit"].as_str(), body["channel_id"].as_str()),
        (Some("345"), Some("12"))
    );
    assert!(body.get("send_at").is_none());

    for bad in [
        "op=send&channel=11&edit=nope&c=x",
        "op=schedule&channel=11&c=x",
    ] {
        let page = html(response(&config, Method::POST, "/g/42/compose", bad).await).await;
        assert!(page.contains("role=\"alert\""), "{bad}");
    }
}

#[tokio::test]
async fn a_setting_that_holds_a_message_is_edited_in_the_composer() {
    let (config, state) = setup().await;
    {
        let mut state = state.lock().unwrap();
        state.forms = true;
        state.structured = true;
    }
    let module = html(response(&config, Method::GET, "/g/42/modules/welcome", "").await).await;
    assert!(module.contains("href=\"/g/42/modules/welcome/compose/message\""));
    let path = "/g/42/modules/welcome/compose/message";
    let page = html(response(&config, Method::GET, path, "").await).await;
    assert!(
        page.contains("name=\"c\"")
            && page.contains("<code>{user}</code>")
            && page.contains("<code>{server}</code>")
    );
    assert!(!page.contains("name=\"channel\""));
    let fragment = html(
        response(
            &config,
            Method::POST,
            &format!("{path}?preview=1"),
            "c=Hi+%7Buser%7D",
        )
        .await,
    )
    .await;
    assert!(fragment.contains("@新しいメンバー"), "{fragment}");

    let save = |body: &'static str| response(&config, Method::POST, path, body);
    assert_eq!(
        save("op=save&etag=%221%22&c=Hi+%7Buser%7D").await.status(),
        StatusCode::SEE_OTHER
    );
    let sent = state.lock().unwrap().form_body.clone().unwrap();
    assert_eq!(sent["modules"]["welcome"]["config"]["message"], "Hi {user}");
    assert_eq!(
        save("op=save&etag=%221%22&c=Hi&e.0.__=1&e.0.title=Welcome+to+%7Bserver%7D")
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
    let sent = state.lock().unwrap().form_body.clone().unwrap();
    assert_eq!(
        sent["modules"]["welcome"]["config"]["message"],
        json!({"content": "Hi", "embeds": [{"title": "Welcome to {server}"}]})
    );
    assert_eq!(
        save("op=save&etag=%221%22").await.status(),
        StatusCode::SEE_OTHER
    );
    let sent = state.lock().unwrap().form_body.clone().unwrap();
    assert!(sent["modules"]["welcome"]["config"]["message"].is_null());
    let other = response(
        &config,
        Method::GET,
        "/g/42/modules/welcome/compose/nothing",
        "",
    )
    .await;
    assert_eq!(other.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn hidden_dependent_role_is_preserved() {
    let (config, state) = setup().await;
    state.lock().unwrap().forms = true;
    let path = "/g/42/modules/moderation";
    let page = html(response(&config, Method::GET, path, "").await).await;
    assert!(page.contains("data-when=\"timeout_method=role\""));
    assert!(page.contains("data-when=\"voice_mute_method=role\""));
    assert!(!page.contains("name=\"f.timeout_method\" value=\"\""));
    let result = response(&config, Method::POST, path,
        "etag=%221%22&f.timeout_method=discord&f.timeout_role=&f.voice_mute_method=discord&f.voice_mute_role=").await;
    assert_eq!(result.status(), StatusCode::SEE_OTHER);
    let state = state.lock().unwrap();
    let config = &state.form_body.as_ref().unwrap()["modules"]["moderation"]["config"];
    assert_eq!(config["timeout_role"], "13");
    assert!(config.get("voice_mute_role").is_none());
}

async fn response(
    config: &Config,
    method: Method,
    path: &str,
    body: &str,
) -> axum::response::Response {
    router(config.clone())
        .oneshot(request(config, method, path, body))
        .await
        .unwrap()
}

async fn html(response: axum::response::Response) -> String {
    String::from_utf8(
        to_bytes(response.into_body(), 131072)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap()
}

#[tokio::test]
async fn list_and_form_render_schema_and_directory() {
    let (config, state) = setup().await;
    state.lock().unwrap().forms = true;
    let page = response(&config, Method::GET, "/g/42/modules", "").await;
    assert_eq!(page.status(), StatusCode::OK);
    let page = html(page).await;
    for expected in [
        "aria-current=\"page\"",
        "aria-checked=\"true\"",
        "/modules/welcome",
    ] {
        assert!(page.contains(expected), "{expected}");
    }
    let page =
        html(response(&config, Method::GET, "/g/42/modules/welcome?saved=1", "").await).await;
    for expected in [
        "role=\"status\"",
        "type=\"checkbox\"",
        "type=\"radio\"",
        "<select",
        "&lt;script&gt;",
        "name=\"etag\"",
    ] {
        assert!(page.contains(expected), "{expected}");
    }
    for forbidden in ["<script>", "style=", ">voice</option>", ">managed</option>"] {
        assert!(!page.contains(forbidden), "{forbidden}");
    }
    let page = html(response(&config, Method::GET, "/g/42/modules/automod", "").await).await;
    for expected in ["type=\"number\"", "min=\"1\"", "max=\"365\""] {
        assert!(page.contains(expected), "{expected}");
    }
    assert!(!page.contains("Automatic moderation: rules"));
    assert!(!page.contains("class=\"muted mono\""));
    let page = html(response(&config, Method::GET, "/g/42/modules/welcome", "").await).await;
    assert!(page.contains(">channel</label>"));
    assert!(!page.contains("class=\"muted mono\""));
    let page = html(response(&config, Method::GET, "/g/42/modules/channels", "").await).await;
    assert!(page.contains("このモジュールに設定項目はありません"));
    state.lock().unwrap().directory_failure = true;
    let page = html(response(&config, Method::GET, "/g/42/modules/welcome", "").await).await;
    assert!(page.contains("type=\"text\""));
    assert!(page.contains("pattern=\"^[0-9]{1,20}$\""));
}

#[tokio::test]
async fn pages_keep_logout_without_preference_controls_and_use_correct_width() {
    let (config, state) = setup().await;
    state.lock().unwrap().forms = true;
    for (path, narrow) in [
        ("/g/42", false),
        ("/g/42/modules", false),
        ("/g/42/history", false),
        ("/g/42/modules/welcome", true),
        ("/g/42/settings", true),
        ("/g/42/permissions", true),
    ] {
        let page = html(response(&config, Method::GET, path, "").await).await;
        assert!(page.contains("action=\"/logout\""), "{path}");
        assert!(
            page.contains(if narrow {
                "class=\"content narrow\""
            } else {
                "class=\"content\""
            }),
            "{path}"
        );
        for forbidden in [
            "breadcrumb",
            "⌄",
            "name=\"lang\"",
            "name=\"theme\"",
            "aria-label=\"言語\"",
            "aria-label=\"テーマ\"",
            "aria-label=\"Language\"",
            "aria-label=\"Theme\"",
        ] {
            assert!(!page.contains(forbidden), "{path}: {forbidden}");
        }
    }
}

#[tokio::test]
async fn save_and_error_paths() {
    let (config, state) = setup().await;
    state.lock().unwrap().forms = true;
    let path = "/g/42/modules/welcome";
    let body = "etag=%221%22&f.channel=11&f.include_bots=false&f.choice=one&f.role=&f.%3Cscript%3E=%3Cscript%3E";
    let saved = response(&config, Method::POST, path, body).await;
    assert_eq!(saved.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        saved.headers()[header::LOCATION],
        "/g/42/modules/welcome?saved=1"
    );
    {
        let state = state.lock().unwrap();
        let body = state.form_body.as_ref().unwrap();
        assert_eq!(body["modules"]["welcome"]["config"]["channel"], "11");
        assert_eq!(
            body["modules"]["welcome"]["config"]["role"],
            serde_json::Value::Null
        );
        assert_eq!(body["language"], "ja");
        assert!(
            body["modules"]["welcome"]["config"]
                .get("include_bots")
                .is_none()
        );
        assert_eq!(
            body["modules"]["logs"]["config"]["channel"],
            "123456789012345678"
        );
        assert!(
            state
                .queries
                .iter()
                .any(|q| q == "/v1/guilds/42/config?view=basic")
        );
        assert!(
            state
                .calls
                .iter()
                .any(|c| c.0.ends_with("/config") && c.2 == "\"1\"" && c.3 == "123")
        );
    }
    state.lock().unwrap().form_put_status = Some(StatusCode::UNPROCESSABLE_ENTITY);
    let invalid = response(&config, Method::POST, path, body).await;
    assert_eq!(invalid.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let page = html(invalid).await;
    for expected in [
        "aria-invalid=\"true\"",
        "role=\"alert\"",
        "&lt;script&gt;bad&lt;/script&gt;",
        "value=\"&lt;script&gt;\"",
        "value=\"11\" selected",
    ] {
        assert!(page.contains(expected), "{expected}");
    }
    assert!(!page.contains("<script>"));
    for status in [
        StatusCode::PRECONDITION_FAILED,
        StatusCode::FORBIDDEN,
        StatusCode::SERVICE_UNAVAILABLE,
    ] {
        state.lock().unwrap().form_put_status = Some(status);
        assert_eq!(
            response(&config, Method::POST, path, body).await.status(),
            status
        );
    }
    state.lock().unwrap().form_put_status = None;
    let stale = response(&config, Method::POST, path, "etag=%222%22").await;
    assert_eq!(stale.status(), StatusCode::PRECONDITION_FAILED);
    assert!(html(stale).await.contains("他の人が変更しました"));
    assert_eq!(
        response(&config, Method::POST, path, "etag=%221%22&f.unknown=x")
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        response(&config, Method::GET, "/g/42/modules/unknown", "")
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        response(&config, Method::POST, path, "etag=%221%22&f.channel=x")
            .await
            .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let anonymous = Request::builder().uri(path).body(Body::empty()).unwrap();
    assert_eq!(
        router(config.clone())
            .oneshot(anonymous)
            .await
            .unwrap()
            .status(),
        StatusCode::SEE_OTHER
    );
    let no_origin = Request::builder()
        .method(Method::POST)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body))
        .unwrap();
    assert_eq!(
        router(config).oneshot(no_origin).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn toggle_returns_only_to_same_origin() {
    let (config, _) = setup().await;
    for (referer, expected) in [
        ("http://localhost/g/42/modules/mod", "/g/42/modules/mod"),
        ("https://evil.example/g/42", "/g/42/modules"),
        ("http://localhost//evil", "/g/42/modules"),
    ] {
        let mut req = request(
            &config,
            Method::POST,
            "/g/42/modules/mod/toggle",
            "enabled=true",
        );
        req.headers_mut()
            .insert(header::REFERER, referer.parse().unwrap());
        let result = router(config.clone()).oneshot(req).await.unwrap();
        assert_eq!(result.status(), StatusCode::SEE_OTHER);
        assert_eq!(result.headers()[header::LOCATION], expected);
    }
}

#[tokio::test]
async fn server_settings_render_and_save() {
    let (config, state) = setup().await;
    state.lock().unwrap().forms = true;
    let path = "/g/42/settings";
    let page = html(response(&config, Method::GET, path, "").await).await;
    for expected in [
        "サーバー設定",
        "name=\"f.language\"",
        "type=\"radio\"",
        "name=\"f.prefix\"",
        "name=\"f.timezone\"",
        ">自動</label>",
        ">日本語</label>",
        ">English</label>",
        "name=\"f.log_channel\"",
        "name=\"f.moderator_roles\"",
        "不明なロール 99",
        "&lt;script&gt;role&lt;/script&gt;",
        "&lt;script&gt;description&lt;/script&gt;",
        "aria-current=\"page\"",
    ] {
        assert!(page.contains(expected), "{expected}");
    }
    for forbidden in [
        "<script>",
        "style=",
        "value=\"42\"",
        ">managed</label>",
        "空で既定に戻ります",
        "class=\"muted mono\"",
        "name=\"f.preset\"",
        "name=\"f.log_webhook\"",
        "name=\"f.success_emoji\"",
        "name=\"f.error_emoji\"",
        "settings-heading",
        ">未設定</label>",
        ">auto</label>",
        ">ja</label>",
        ">en</label>",
    ] {
        assert!(!page.contains(forbidden), "{forbidden}");
    }
    assert_eq!(page.matches("name=\"f.language\"").count(), 3);
    assert!(page.contains("title=\"&lt;script&gt;description&lt;/script&gt;\""));
    let body = "etag=%221%22&f.language=ja&f.timezone=Asia%2FTokyo&f.prefix=%21&f.log_channel=11&roles_present=1&f.moderator_roles=13&f.moderator_roles=99";
    let result = response(&config, Method::POST, path, body).await;
    assert_eq!(result.status(), StatusCode::SEE_OTHER);
    assert_eq!(result.headers()[header::LOCATION], "/g/42/settings?saved=1");
    {
        let state = state.lock().unwrap();
        let body = state.form_body.as_ref().unwrap();
        assert_eq!(*body, json!({"log_channel":"11"}));
        assert!(
            state
                .queries
                .iter()
                .any(|q| q == "/v1/guilds/42/config?view=basic")
        );
        assert!(
            state
                .calls
                .iter()
                .any(|c| c.0.ends_with("/config") && c.2 == "\"1\"" && c.3 == "123")
        );
    }
    assert_eq!(
        response(
            &config,
            Method::POST,
            path,
            "etag=%221%22&roles_present=1&f.moderator_roles=13&f.moderator_roles=77"
        )
        .await
        .status(),
        StatusCode::SEE_OTHER
    );
    assert_eq!(
        state.lock().unwrap().form_body.as_ref().unwrap()["moderator_roles"],
        json!(["13", "77"])
    );
    assert_eq!(
        response(
            &config,
            Method::POST,
            path,
            "etag=%221%22&roles_present=1&f.prefix="
        )
        .await
        .status(),
        StatusCode::SEE_OTHER
    );
    assert_eq!(
        *state.lock().unwrap().form_body.as_ref().unwrap(),
        json!({"moderator_roles":null,"prefix":null})
    );
    state.lock().unwrap().directory_failure = true;
    let page = html(response(&config, Method::GET, path, "").await).await;
    assert!(page.contains("name=\"f.moderator_roles\""));
    assert!(page.contains("value=\"13,99\""));
    assert_eq!(
        response(
            &config,
            Method::POST,
            path,
            "etag=%221%22&f.moderator_roles=13%2C77"
        )
        .await
        .status(),
        StatusCode::SEE_OTHER
    );
    assert_eq!(
        state.lock().unwrap().form_body.as_ref().unwrap()["moderator_roles"],
        json!(["13", "77"])
    );
}

#[tokio::test]
async fn server_settings_errors_and_security() {
    let (config, state) = setup().await;
    state.lock().unwrap().forms = true;
    let path = "/g/42/settings";
    let body = "etag=%221%22&f.timezone=%3Cscript%3E";
    state.lock().unwrap().form_put_status = Some(StatusCode::UNPROCESSABLE_ENTITY);
    let result = response(&config, Method::POST, path, body).await;
    assert_eq!(result.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let page = html(result).await;
    for expected in [
        "aria-invalid=\"true\"",
        "&lt;script&gt;bad&lt;/script&gt;",
        "value=\"&lt;script&gt;\"",
    ] {
        assert!(page.contains(expected), "{expected}");
    }
    state.lock().unwrap().form_put_status = Some(StatusCode::FORBIDDEN);
    let forbidden = response(&config, Method::POST, path, body).await;
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
    assert!(html(forbidden).await.contains("アクセス権がありません"));
    state.lock().unwrap().form_put_status = Some(StatusCode::PRECONDITION_FAILED);
    assert!(
        html(response(&config, Method::POST, path, body).await)
            .await
            .contains("他の人が変更しました")
    );
    assert_eq!(
        response(&config, Method::POST, path, "etag=%222%22")
            .await
            .status(),
        StatusCode::PRECONDITION_FAILED
    );
    assert_eq!(
        response(&config, Method::POST, path, "etag=%221%22&f.unknown=x")
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        response(
            &config,
            Method::POST,
            path,
            "etag=%221%22&f.moderator_roles=bad"
        )
        .await
        .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let anonymous = Request::builder().uri(path).body(Body::empty()).unwrap();
    assert_eq!(
        router(config.clone())
            .oneshot(anonymous)
            .await
            .unwrap()
            .status(),
        StatusCode::SEE_OTHER
    );
    let no_origin = Request::builder()
        .method(Method::POST)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body))
        .unwrap();
    assert_eq!(
        router(config).oneshot(no_origin).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
}
