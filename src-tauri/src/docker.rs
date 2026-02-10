use std::collections::HashMap;

use bollard::container::{
    Config, CreateContainerOptions, RemoveContainerOptions, StartContainerOptions,
    StopContainerOptions,
};
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::image::CreateImageOptions;
use bollard::models::{HostConfig, Mount, MountTypeEnum, PortBinding};
use bollard::Docker;
use futures_util::StreamExt;

use crate::error::{BunyanError, Result};

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
) -> Result<String> {
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
    let mut mounts = vec![
        Mount {
            target: Some("/workspace".to_string()),
            source: Some(workspace_path.to_string()),
            typ: Some(MountTypeEnum::BIND),
            ..Default::default()
        },
        Mount {
            target: Some("/home/dev/.claude".to_string()),
            source: Some(home.join(".claude").to_string_lossy().to_string()),
            typ: Some(MountTypeEnum::BIND),
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

    // Build port bindings
    let mut exposed_ports = HashMap::new();
    let mut port_bindings: HashMap<String, Option<Vec<PortBinding>>> = HashMap::new();
    for port_spec in ports {
        // Format: "host_port:container_port"
        if let Some((host_port, container_port)) = port_spec.split_once(':') {
            let key = format!("{}/tcp", container_port);
            exposed_ports.insert(key.clone(), HashMap::new());
            port_bindings.insert(
                key,
                Some(vec![PortBinding {
                    host_ip: Some("0.0.0.0".to_string()),
                    host_port: Some(host_port.to_string()),
                }]),
            );
        }
    }

    let host_config = HostConfig {
        mounts: Some(mounts),
        port_bindings: Some(port_bindings),
        ..Default::default()
    };

    let config = Config {
        image: Some(image.to_string()),
        cmd: Some(vec!["sleep".to_string(), "infinity".to_string()]),
        working_dir: Some("/workspace".to_string()),
        env: Some(env.to_vec()),
        exposed_ports: Some(exposed_ports),
        host_config: Some(host_config),
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

/// Build the `docker exec` command string for a tmux pane.
pub fn docker_exec_cmd(container_id: &str, cmd: &str) -> String {
    format!("docker exec -it {} {}", container_id, cmd)
}
