/// Convert a forward-slash path (e.g. `"myapp/db"`) to a Kubernetes-safe name (`"myapp-db"`).
///
/// Used by Azure Key Vault, GCP Secret Manager, and OCP backends which all require names
/// without `/`.
#[allow(dead_code)]
pub fn path_to_k8s_name(path: &str) -> String {
    path.replace('/', "-")
}

/// Reverse of [`path_to_k8s_name`]: convert `"myapp-db"` back to `"myapp/db"`.
#[allow(dead_code)]
pub fn k8s_name_to_path(name: &str) -> String {
    name.replace('-', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let path = "myapp/database/primary";
        assert_eq!(k8s_name_to_path(&path_to_k8s_name(path)), path);
    }

    #[test]
    fn no_slashes_unchanged() {
        assert_eq!(path_to_k8s_name("mysecret"), "mysecret");
        assert_eq!(k8s_name_to_path("mysecret"), "mysecret");
    }
}
