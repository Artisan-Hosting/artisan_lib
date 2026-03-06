#[cfg(test)]
mod tests {
    use crate::enviornment::definitions::{
        ApplicationType, Enviornment, Enviornment_V1, ExecutionUser,
    };
    use crate::git_actions::{GitServer, ARTISANCF};
    use dusa_collection_utils::core::{logger::LogLevel, types::stringy::Stringy};

    #[tokio::test]
    async fn v1_parse_to_parse_from_roundtrip() {
        let v1 = Enviornment_V1 {
            application_type: Some(ApplicationType::Python),
            execution_uid: Some(1000),
            execution_gid: Some(1000),
            primary_listening_port: Some(8080),
            secret_id: Some(Stringy::from("secret-id")),
            secret_passwd: Some(Stringy::from("secret-password")),
            path_modifier: Some(Stringy::from("/opt/artisan/bin")),
            pre_build_command: Some(Stringy::from("pip install -r requirements.txt")),
            build_command: Some(Stringy::from("python -m compileall .")),
            run_command: Some(Stringy::from("python app.py")),
            env_key_0: Some((Stringy::from("APP_ENV"), Stringy::from("development"))),
        };

        let encoded = v1.parse_to().await.unwrap();
        let decoded = Enviornment_V1::parse_from(&encoded).await.unwrap();
        assert_eq!(decoded, v1);
    }

    #[tokio::test]
    async fn v1_parse_to_enviornment_enum_roundtrip() {
        let v1 = Enviornment_V1 {
            application_type: Some(ApplicationType::Simple),
            execution_uid: Some(33),
            execution_gid: Some(33),
            primary_listening_port: Some(3000),
            secret_id: None,
            secret_passwd: None,
            path_modifier: None,
            pre_build_command: None,
            build_command: Some(Stringy::from("cargo build --release")),
            run_command: Some(Stringy::from("./target/release/app")),
            env_key_0: Some((Stringy::from("RUST_LOG"), Stringy::from("info"))),
        };

        let encoded = v1.parse_to().await.unwrap();
        let decoded = Enviornment::parse(&encoded).await.unwrap();
        assert_eq!(decoded, Enviornment::V1(v1));
    }

    #[test]
    fn v2_new_finalize_minimal_is_valid() {
        let env = Enviornment::new_v2().finalize();
        assert!(matches!(env, Ok(Enviornment::V2(_))));
    }

    #[test]
    fn v2_finalize_rejects_invalid_port_range() {
        let env = Enviornment::new_v2().with_port_range(9001, 8001).finalize();
        assert!(env.is_err());
    }

    #[test]
    fn v2_builder_sets_fields_and_finalizes() {
        let env = Enviornment::new_v2()
            .with_max_ram_usage(512)
            .with_max_cpu_usage(80)
            .with_debug_mode(true)
            .with_log_level(LogLevel::Debug)
            .with_git_config(crate::config::GitConfig {
                default_server: GitServer::GitHub,
                credentials_file: ARTISANCF.to_string(),
            })
            .with_execution_user(ExecutionUser::Custom(1001, 1001))
            .with_port_range(3000, 3010)
            .with_secret("SECRET_KEY", "secret-value")
            .with_env_var("APP_ENV", "development")
            .with_dependency_command("npm ci")
            .with_build_command("npm run build")
            .with_run_command("npm run start")
            .finalize();

        let Enviornment::V2(v2) = env.unwrap() else {
            panic!("Expected Enviornment::V2");
        };

        assert_eq!(v2.max_ram_usage, Some(512));
        assert_eq!(v2.max_cpu_usage, Some(80));
        assert!(v2.debug_mode);
        assert_eq!(v2.log_level, LogLevel::Debug);
        assert_eq!(v2.port_range, Some((3000, 3010)));
        assert_eq!(v2.secret_store.unwrap().len(), 1);
        assert_eq!(v2.env_var_store.unwrap().len(), 1);
    }

    #[test]
    fn v2_mutable_setter_style_works() {
        let mut env = Enviornment::new_v2();
        env.set_max_ram_usage(1024);
        env.set_max_cpu_usage(95);
        env.set_debug_mode(true);
        env.set_execution_user(ExecutionUser::Artisan);
        env.set_port_range(4000, 4010);
        env.add_secret("TOKEN", "abcd");
        env.add_env_var("RUST_LOG", "info");
        env.set_run_command("cargo run --release");

        let Enviornment::V2(v2) = env.finalize().unwrap() else {
            panic!("Expected Enviornment::V2");
        };

        assert_eq!(v2.max_ram_usage, Some(1024));
        assert_eq!(v2.max_cpu_usage, Some(95));
        assert!(v2.debug_mode);
        assert_eq!(v2.execution_user, ExecutionUser::Artisan);
        assert_eq!(v2.port_range, Some((4000, 4010)));
        assert_eq!(v2.secret_store.unwrap().len(), 1);
        assert_eq!(v2.env_var_store.unwrap().len(), 1);
    }
}
