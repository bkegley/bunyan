use std::collections::HashMap;

use bollard::container::{
    Config, CreateContainerOptions, RemoveContainerOptions, StartContainerOptions,
    StopContainerOptions,
};
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::image::CreateImageOptions;
use bollard::models::{HostConfig, Mount, MountTypeEnum, PortBinding};
use bollard::network::CreateNetworkOptions;
use bollard::Docker;
use futures_util::StreamExt;

use crate::error::{BunyanError, Result};
use crate::models::PortMapping;

/// Allowed base image prefixes. Images must start with one of these.
/// Covers official Docker Hub images and common trusted registries.
const ALLOWED_IMAGE_PREFIXES: &[&str] = &[
    "node:",
    "ubuntu:",
    "debian:",
    "alpine:",
    "python:",
    "rust:",
    "golang:",
    "mcr.microsoft.com/",
    "ghcr.io/",
    // Also allow bare names (e.g. "node" without tag)
    "node",
    "ubuntu",
    "debian",
    "alpine",
    "python",
    "rust",
    "golang",
];

/// Validate that a Docker image is from a trusted source.
pub fn validate_image(image: &str) -> Result<()> {
    if image.is_empty() {
        return Err(BunyanError::Docker("Empty image name".to_string()));
    }
    // Reject images with shell metacharacters
    if image.chars().any(|c| matches!(c, ';' | '&' | '|' | '$' | '`' | '\'' | '"' | '\\' | '\n')) {
        return Err(BunyanError::Docker(format!("Image name contains invalid characters: {}", image)));
    }
    let is_allowed = ALLOWED_IMAGE_PREFIXES.iter().any(|prefix| image.starts_with(prefix));
    if !is_allowed {
        return Err(BunyanError::Docker(format!(
            "Image '{}' is not in the allowlist. Allowed: node, ubuntu, debian, alpine, python, rust, golang, mcr.microsoft.com/*, ghcr.io/*",
            image
        )));
    }
    Ok(())
}

/// Environment variable names that are blocked from being passed to containers.
const BLOCKED_ENV_VARS: &[&str] = &[
    "LD_PRELOAD",
    "LD_LIBRARY_PATH",
    "PATH",
    "HOME",
    "USER",
    "SHELL",
    "HOSTNAME",
    "DOCKER_HOST",
];

/// Validate environment variables, rejecting dangerous overrides.
pub fn validate_env(env: &[String]) -> Result<()> {
    for entry in env {
        if let Some(key) = entry.split('=').next() {
            let upper = key.to_uppercase();
            if BLOCKED_ENV_VARS.contains(&upper.as_str()) {
                return Err(BunyanError::Docker(format!(
                    "Environment variable '{}' is not allowed (security-sensitive)",
                    key
                )));
            }
        }
    }
    Ok(())
}

/// Sanitize a string for use as a Docker container or network name.
/// Replaces invalid characters with dashes and ensures it starts with alphanumeric.
pub fn sanitize_docker_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-' { c } else { '-' })
        .collect();
    // Ensure it starts with alphanumeric
    if sanitized.starts_with(|c: char| !c.is_ascii_alphanumeric()) {
        format!("x{}", sanitized)
    } else {
        sanitized
    }
}

/// Check if the Docker daemon is reachable.
pub async fn check_docker() -> Result<bool> {
    let docker = match Docker::connect_with_local_defaults() {
        Ok(d) => d,
        Err(_) => return Ok(false),
    };
    match docker.ping().await {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}

/// Create and start a container for a workspace.
/// Returns the container ID.
pub async fn create_workspace_container(
    image: &str,
    workspace_path: &str,
    container_name: &str,
    ports: &[String],
    env: &[String],
    network_name: Option<&str>,
    directory_name: &str,
) -> Result<String> {
    validate_image(image)?;
    validate_env(env)?;

    let docker = Docker::connect_with_local_defaults()?;

    // Pull image if not available locally
    let images = docker
        .list_images::<String>(None)
        .await?;
    let has_image = images.iter().any(|img| {
        img.repo_tags
            .iter()
            .any(|t| t == image || t == &format!("{}:latest", image))
    });

    if !has_image {
        let pull_image = if image.contains(':') {
            image.to_string()
        } else {
            format!("{}:latest", image)
        };
        let (repo, tag) = pull_image
            .rsplit_once(':')
            .unwrap_or((&pull_image, "latest"));

        let mut stream = docker.create_image(
            Some(CreateImageOptions {
                from_image: repo.to_string(),
                tag: tag.to_string(),
                ..Default::default()
            }),
            None,
            None,
        );
        while let Some(result) = stream.next().await {
            result?;
        }
    }

    // Build mounts
    let home = dirs::home_dir().ok_or_else(|| BunyanError::Docker("Cannot determine home directory".to_string()))?;
    let mount_target = format!("/workspace/{}", directory_name);
    let mut mounts = vec![
        Mount {
            target: Some(mount_target.clone()),
            source: Some(workspace_path.to_string()),
            typ: Some(MountTypeEnum::BIND),
            ..Default::default()
        },
        Mount {
            target: Some("/home/dev/.claude".to_string()),
            source: Some(home.join(".claude").to_string_lossy().to_string()),
            typ: Some(MountTypeEnum::BIND),
            read_only: Some(true),
            ..Default::default()
        },
        Mount {
            target: Some("/home/dev/.ssh".to_string()),
            source: Some(home.join(".ssh").to_string_lossy().to_string()),
            typ: Some(MountTypeEnum::BIND),
            read_only: Some(true),
            ..Default::default()
        },
    ];

    let gitconfig = home.join(".gitconfig");
    if gitconfig.exists() {
        mounts.push(Mount {
            target: Some("/home/dev/.gitconfig".to_string()),
            source: Some(gitconfig.to_string_lossy().to_string()),
            typ: Some(MountTypeEnum::BIND),
            read_only: Some(true),
            ..Default::default()
        });
    }

    // Build port bindings (validated)
    let mut exposed_ports = HashMap::new();
    let mut port_bindings: HashMap<String, Option<Vec<PortBinding>>> = HashMap::new();
    for port_spec in ports {
        // Format: "host_port:container_port"
        if let Some((host_port, container_port)) = port_spec.split_once(':') {
            let hp: u16 = host_port.parse().map_err(|_| {
                BunyanError::Docker(format!("Invalid host port: {}", host_port))
            })?;
            let cp: u16 = container_port.parse().map_err(|_| {
                BunyanError::Docker(format!("Invalid container port: {}", container_port))
            })?;
            if hp < 1024 {
                return Err(BunyanError::Docker(format!(
                    "Host port {} is privileged (< 1024). Use a port >= 1024.",
                    hp
                )));
            }
            if cp == 0 {
                return Err(BunyanError::Docker("Container port cannot be 0".to_string()));
            }
            let key = format!("{}/tcp", cp);
            exposed_ports.insert(key.clone(), HashMap::new());
            port_bindings.insert(
                key,
                Some(vec![PortBinding {
                    host_ip: Some("127.0.0.1".to_string()),
                    host_port: Some(hp.to_string()),
                }]),
            );
        }
    }

    let host_config = HostConfig {
        mounts: Some(mounts),
        port_bindings: Some(port_bindings),
        network_mode: network_name.map(|n| n.to_string()),
        // Resource limits to prevent DoS
        nano_cpus: Some(4_000_000_000),   // 4 CPU cores
        memory: Some(8 * 1024 * 1024 * 1024), // 8 GB
        pids_limit: Some(512),
        ..Default::default()
    };

    let config = Config {
        image: Some(image.to_string()),
        cmd: Some(vec!["sleep".to_string(), "infinity".to_string()]),
        working_dir: Some(mount_target),
        env: Some(env.to_vec()),
        exposed_ports: Some(exposed_ports),
        host_config: Some(host_config),
        user: Some("1000:1000".to_string()),
        ..Default::default()
    };

    let container = docker
        .create_container(
            Some(CreateContainerOptions {
                name: container_name.to_string(),
                ..Default::default()
            }),
            config,
        )
        .await?;

    docker
        .start_container(&container.id, None::<StartContainerOptions<String>>)
        .await?;

    Ok(container.id)
}

/// Stop and remove a container.
pub async fn remove_container(container_id: &str) -> Result<()> {
    let docker = Docker::connect_with_local_defaults()?;

    // Stop (ignore errors — container may already be stopped)
    let _ = docker
        .stop_container(container_id, Some(StopContainerOptions { t: 5 }))
        .await;

    // Remove
    docker
        .remove_container(
            container_id,
            Some(RemoveContainerOptions {
                force: true,
                ..Default::default()
            }),
        )
        .await?;

    Ok(())
}

/// Ensure Claude CLI is available in the container.
/// Checks for `claude`, installs via npm if not found.
pub async fn ensure_claude(container_id: &str) -> Result<()> {
    let docker = Docker::connect_with_local_defaults()?;

    // Check if claude is available
    let exec = docker
        .create_exec(
            container_id,
            CreateExecOptions {
                cmd: Some(vec!["which".to_string(), "claude".to_string()]),
                attach_stdout: Some(true),
                attach_stderr: Some(true),
                ..Default::default()
            },
        )
        .await?;

    let result = docker.start_exec(&exec.id, None).await?;
    let mut found = false;
    if let StartExecResults::Attached { mut output, .. } = result {
        while let Some(Ok(_)) = output.next().await {}
    }

    // Check exit code
    let inspect = docker.inspect_exec(&exec.id).await?;
    if inspect.exit_code == Some(0) {
        found = true;
    }

    if !found {
        // Install claude via npm
        let exec = docker
            .create_exec(
                container_id,
                CreateExecOptions {
                    cmd: Some(vec![
                        "npm".to_string(),
                        "install".to_string(),
                        "-g".to_string(),
                        "@anthropic-ai/claude-code".to_string(),
                    ]),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    ..Default::default()
                },
            )
            .await?;

        let result = docker.start_exec(&exec.id, None).await?;
        if let StartExecResults::Attached { mut output, .. } = result {
            while let Some(Ok(_)) = output.next().await {}
        }

        let inspect = docker.inspect_exec(&exec.id).await?;
        if inspect.exit_code != Some(0) {
            return Err(BunyanError::Docker(
                "Failed to install Claude CLI in container (npm install failed)".to_string(),
            ));
        }
    }

    Ok(())
}

/// Get the status of a container: "running", "stopped", or "none".
pub async fn get_container_status(container_id: &str) -> Result<String> {
    let docker = Docker::connect_with_local_defaults()?;
    match docker.inspect_container(container_id, None).await {
        Ok(info) => {
            let running = info
                .state
                .and_then(|s| s.running)
                .unwrap_or(false);
            Ok(if running {
                "running".to_string()
            } else {
                "stopped".to_string()
            })
        }
        Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 404, ..
        }) => Ok("none".to_string()),
        Err(e) => Err(e.into()),
    }
}

/// Create a Docker bridge network. Idempotent — ignores "already exists" errors.
pub async fn create_network(network_name: &str) -> Result<()> {
    let docker = Docker::connect_with_local_defaults()?;

    let config = CreateNetworkOptions {
        name: network_name,
        driver: "bridge",
        ..Default::default()
    };

    match docker.create_network(config).await {
        Ok(_) => Ok(()),
        Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 409, ..
        }) => Ok(()), // network already exists
        Err(e) => Err(e.into()),
    }
}

/// Remove a Docker network. Idempotent — ignores 404.
pub async fn remove_network(network_name: &str) -> Result<()> {
    let docker = Docker::connect_with_local_defaults()?;

    match docker.remove_network(network_name).await {
        Ok(_) => Ok(()),
        Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 404, ..
        }) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Get port mappings for a running container.
pub async fn get_container_ports(container_id: &str) -> Result<Vec<PortMapping>> {
    let docker = Docker::connect_with_local_defaults()?;
    let info = docker.inspect_container(container_id, None).await?;

    let mut mappings = Vec::new();

    if let Some(network_settings) = info.network_settings {
        if let Some(ports) = network_settings.ports {
            for (container_port, bindings) in ports {
                if let Some(bindings) = bindings {
                    for binding in bindings {
                        mappings.push(PortMapping {
                            container_port: container_port.clone(),
                            host_port: binding.host_port.unwrap_or_default(),
                            host_ip: binding.host_ip.unwrap_or_else(|| "0.0.0.0".to_string()),
                        });
                    }
                }
            }
        }
    }

    Ok(mappings)
}

/// Shell-escape a string for safe inclusion in a shell command.
/// Wraps in single quotes and escapes embedded single quotes.
fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Validate that a string is a safe Docker container ID (hex hash or name).
fn validate_container_id(id: &str) -> Result<()> {
    if id.is_empty() {
        return Err(BunyanError::Docker("Empty container ID".to_string()));
    }
    // Docker container IDs are hex strings; names match [a-zA-Z0-9][a-zA-Z0-9_.-]
    let is_valid = id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-');
    if !is_valid {
        return Err(BunyanError::Docker(format!("Invalid container ID: {}", id)));
    }
    Ok(())
}

/// Build the `docker exec` command string for a tmux pane.
/// Shell-escapes both container_id and cmd to prevent injection.
pub fn docker_exec_cmd(container_id: &str, cmd: &str) -> Result<String> {
    validate_container_id(container_id)?;
    Ok(format!("docker exec -it {} {}", shell_escape(container_id), cmd))
}
