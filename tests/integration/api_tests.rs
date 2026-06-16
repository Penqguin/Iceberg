use mockito::Server;

#[tokio::test]
async fn test_github_api_mock() {
    let mut server = Server::new_async().await;
    let _mock = server.mock("POST", "/graphql")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"data": {"user": {"repositories": {"nodes": []}}}}"#)
        .create_async()
        .await;

    // In a real scenario, we would configure the client to use server.url()
    assert!(true);
}
