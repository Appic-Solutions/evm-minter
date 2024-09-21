mod api_key {
    use crate::rpc_client::providers::Provider;
    use crate::storage::{get_rpc_api_key, set_rpc_api_key};
    #[test]
    fn should_set_get_api_key() {
        set_rpc_api_key(Provider::Alchemy, "Test_key_Alchemy".to_string());
        set_rpc_api_key(Provider::Ankr, "Test_key_Ankr".to_string());

        assert_eq!(
            get_rpc_api_key(Provider::Alchemy),
            Some("Test_key_Alchemy".to_string())
        );
        assert_eq!(
            get_rpc_api_key(Provider::Ankr),
            Some("Test_key_Ankr".to_string())
        );

        assert_eq!(get_rpc_api_key(Provider::PublicNode), None);
    }
    #[test]
    fn should_update_api_key() {
        set_rpc_api_key(Provider::Alchemy, "Test_key_Alchemy".to_string());

        set_rpc_api_key(Provider::Alchemy, "Test_key_updated_Alchemy".to_string());

        assert_eq!(
            get_rpc_api_key(Provider::Alchemy),
            Some("Test_key_updated_Alchemy".to_string())
        );
    }
}