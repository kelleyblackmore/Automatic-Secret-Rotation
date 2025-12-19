use secret_rotator::backends::{AwsSecretsClient, create_test_client};
use aws_sdk_secretsmanager::types::Tag;
use std::collections::HashMap;

#[test]
fn test_tags_to_metadata() {
    let client = AwsSecretsClient {
        client: create_test_client(),
        region: "us-east-1".to_string(),
    };

    let tags = vec![
        Tag::builder().key("rotation_enabled").value("true").build(),
        Tag::builder()
            .key("last_rotated")
            .value("2023-01-01T00:00:00Z")
            .build(),
        Tag::builder()
            .key("target_username")
            .value("testuser")
            .build(),
    ];

    let metadata = client.tags_to_metadata(&tags);
    assert_eq!(metadata.get("rotation_enabled"), Some(&"true".to_string()));
    assert_eq!(
        metadata.get("last_rotated"),
        Some(&"2023-01-01T00:00:00Z".to_string())
    );
    assert_eq!(
        metadata.get("target_username"),
        Some(&"testuser".to_string())
    );
}

#[test]
fn test_tags_to_metadata_empty() {
    let client = AwsSecretsClient {
        client: create_test_client(),
        region: "us-east-1".to_string(),
    };

    let tags = vec![];
    let metadata = client.tags_to_metadata(&tags);
    assert!(metadata.is_empty());
}

#[test]
fn test_metadata_to_tags() {
    let client = AwsSecretsClient {
        client: create_test_client(),
        region: "us-east-1".to_string(),
    };

    let mut metadata = HashMap::new();
    metadata.insert("rotation_enabled".to_string(), "true".to_string());
    metadata.insert(
        "last_rotated".to_string(),
        "2023-01-01T00:00:00Z".to_string(),
    );
    metadata.insert("target_username".to_string(), "testuser".to_string());

    let tags = client.metadata_to_tags(&metadata);
    assert_eq!(tags.len(), 3);

    // Verify tag values
    let tag_map: HashMap<String, String> = tags
        .iter()
        .filter_map(|tag| {
            tag.key()
                .and_then(|k| tag.value().map(|v| (k.to_string(), v.to_string())))
        })
        .collect();

    assert_eq!(tag_map.get("rotation_enabled"), Some(&"true".to_string()));
    assert_eq!(
        tag_map.get("last_rotated"),
        Some(&"2023-01-01T00:00:00Z".to_string())
    );
    assert_eq!(
        tag_map.get("target_username"),
        Some(&"testuser".to_string())
    );
}

#[test]
fn test_metadata_to_tags_empty() {
    let client = AwsSecretsClient {
        client: create_test_client(),
        region: "us-east-1".to_string(),
    };

    let metadata = HashMap::new();
    let tags = client.metadata_to_tags(&metadata);
    assert!(tags.is_empty());
}

