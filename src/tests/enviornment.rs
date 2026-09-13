#[cfg(test)]
mod tests {
    use crate::enviornment::definitions::{Enviornment, Enviornment_V1, Enviornment_V2};
    use dusa_collection_utils::core::{logger::LogLevel, types::stringy::Stringy};

    fn sample_v1() -> Enviornment_V1 {
        Enviornment_V1 {
            application_type: None,
            execution_uid: Some(33),
            execution_gid: Some(33),
            primary_listening_port: Some(8080),
            secret_id: None,
            secret_passwd: None,
            path_modifier: None,
            pre_build_command: None,
            build_command: Some(Stringy::from("npm run build")),
            run_command: Some(Stringy::from("npm start")),
            env_key_0: Some((Stringy::from("KEY"), Stringy::from("value"))),
        }
    }

    fn sample_v2() -> Enviornment_V2 {
        Enviornment_V2 {
            app_name: Stringy::from("demo"),
            max_ram_usage: 512,
            max_cpu_usage: 0,
            environment: Stringy::from("production"),
            debug_mode: false,
            log_level: LogLevel::Info,
            git: None,
            database: None,
            aggregator: None,
            interval_seconds: 30,
            monitor_path: Stringy::from("/opt/artisan/src/demo"),
            project_path: Stringy::from("/opt/artisan/src/demo"),
            changes_needed: 1,
            ignored_subdirs: vec![Stringy::from(".git")],
            install_command: None,
            build_command: None,
            run_command: Stringy::from("node server.js"),
            application_type: None,
            execution_uid: Some(33),
            execution_gid: Some(33),
            primary_listening_port: Some(3000),
            path_modifier: None,
            pre_build_command: None,
        }
    }

    /// E1: `Enviornment_V1`'s own encrypt/parse round trip must keep working
    /// unmodified.
    #[tokio::test]
    async fn v1_round_trips_through_its_own_envelope() {
        let original = sample_v1();
        let encrypted = original.parse_to().await.unwrap();
        let restored = Enviornment_V1::parse_from(&encrypted).await.unwrap();

        assert_eq!(restored.execution_uid, original.execution_uid);
        assert_eq!(restored.run_command, original.run_command);
        assert_eq!(restored.env_key_0, original.env_key_0);
    }

    /// E1: the enum-level dispatcher must still resolve V1 correctly.
    #[tokio::test]
    async fn enviornment_parse_dispatches_v1() {
        let encrypted = sample_v1().parse_to().await.unwrap();
        match Enviornment::parse(&encrypted).await.unwrap() {
            Enviornment::V1(v1) => assert_eq!(v1.execution_uid, Some(33)),
            Enviornment::V2(_) => panic!("expected V1"),
        }
    }

    /// E2: `Enviornment_V2`'s decode path was a literal `unimplemented!()`
    /// before this phase -- confirm it now round-trips.
    #[tokio::test]
    async fn v2_round_trips_through_its_own_envelope() {
        let original = sample_v2();
        let encrypted = original.parse_to().await.unwrap();
        let restored = Enviornment_V2::parse_from(&encrypted).await.unwrap();

        assert_eq!(restored.app_name, original.app_name);
        assert_eq!(restored.max_ram_usage, original.max_ram_usage);
        assert_eq!(restored.run_command, original.run_command);
        assert_eq!(restored.execution_uid, original.execution_uid);
    }

    /// E2: the enum-level dispatcher must resolve V2 too, now that it's real.
    #[tokio::test]
    async fn enviornment_parse_dispatches_v2() {
        let encrypted = sample_v2().parse_to().await.unwrap();
        match Enviornment::parse(&encrypted).await.unwrap() {
            Enviornment::V2(v2) => assert_eq!(v2.app_name.to_string(), "demo"),
            Enviornment::V1(_) => panic!("expected V2"),
        }
    }

    /// E2: `Enviornment_V2` is usable as plain serde (TOML-shaped) content
    /// too, since the runtime bundle stores it as a plain file inside an
    /// already-encrypted acai container rather than through this envelope.
    #[test]
    fn v2_round_trips_as_plain_toml() {
        let original = sample_v2();
        let toml_text = toml::to_string(&original).expect("serialize to toml");
        let restored: Enviornment_V2 = toml::from_str(&toml_text).expect("parse from toml");

        assert_eq!(restored.app_name, original.app_name);
        assert_eq!(restored.ignored_subdirs, original.ignored_subdirs);
    }
}
