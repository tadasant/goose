use goose_sdk_types::custom_requests::{
    DiscoveryCardDto, DiscoveryDismissRequest, DiscoveryInstallRequest, DiscoveryInstallResponse,
    DiscoveryPendingRequest, DiscoveryPendingResponse, DiscoverySkillEntry, EmptyResponse,
};

use super::{GooseAcpAgent, ResultExt};
use crate::discovery::{discovery_to_extension, Discovery, DiscoveryKind};

fn discovery_card(d: &Discovery) -> DiscoveryCardDto {
    let card = d.server_card();
    let remote = card.and_then(|c| c.primary_remote());
    let auth = remote.and_then(|r| r.auth_requirement());
    let skills = match &d.kind {
        DiscoveryKind::Skills(b) => b
            .skills
            .iter()
            .map(|s| DiscoverySkillEntry {
                name: s.name.clone(),
                description: s.description.clone(),
            })
            .collect(),
        DiscoveryKind::Server(_) => Vec::new(),
    };
    DiscoveryCardDto {
        id: d.id.clone(),
        kind: if d.is_server() { "server" } else { "skills" }.to_string(),
        title: d.title(),
        description: d.description().to_string(),
        source_domain: d.source_domain.clone(),
        fixture: d.fixture,
        // Remote icon URLs would be fetched by the renderer (a tracking pixel); inline only.
        icon: card
            .and_then(|c| c.icon_url())
            .filter(|u| u.starts_with("data:"))
            .map(str::to_string),
        requires_auth: d.requires_auth(),
        auth_kind: auth.as_ref().map(|a| a.kind.clone()),
        auth_instructions: auth.and_then(|a| a.instructions),
        server_name: card.map(|c| c.name.clone()),
        version: card.and_then(|c| c.version.clone()),
        endpoint: remote.map(|r| r.url.clone()),
        skills,
    }
}

impl GooseAcpAgent {
    pub(super) async fn on_discovery_pending(
        &self,
        req: DiscoveryPendingRequest,
    ) -> Result<DiscoveryPendingResponse, agent_client_protocol::Error> {
        let agent = self.get_session_agent(&req.session_id).await?;
        Ok(DiscoveryPendingResponse {
            mode: agent.discovery.mode().as_str().to_string(),
            discoveries: agent
                .discovery
                .pending()
                .iter()
                .map(discovery_card)
                .collect(),
        })
    }

    pub(super) async fn on_discovery_install(
        &self,
        req: DiscoveryInstallRequest,
    ) -> Result<DiscoveryInstallResponse, agent_client_protocol::Error> {
        let agent = self.get_session_agent(&req.session_id).await?;
        let discovery = agent.discovery.get(&req.id).ok_or_else(|| {
            agent_client_protocol::Error::invalid_params()
                .data(format!("Unknown discovery id: {}", req.id))
        })?;

        if !discovery.is_server() {
            agent.discovery.mark_installed(&req.id);
            return Ok(DiscoveryInstallResponse {
                kind: "skills".to_string(),
                extension_name: None,
                tools: Vec::new(),
                message: format!("Skills bundle \"{}\" acknowledged", discovery.title()),
            });
        }

        let config = discovery_to_extension(&discovery, req.secret.as_deref())
            .map_err(|e| agent_client_protocol::Error::invalid_params().data(e))?;
        let ext_name = config.name();
        if agent
            .list_extensions(&req.session_id)
            .await
            .internal_err()?
            .contains(&ext_name)
        {
            return Err(agent_client_protocol::Error::invalid_params().data(format!(
                "An extension named `{ext_name}` is already active; not replacing it"
            )));
        }
        agent
            .add_extension(config, &req.session_id)
            .await
            .internal_err()?;
        agent.discovery.mark_installed(&req.id);

        let tools = agent
            .list_tools(&req.session_id, Some(ext_name.clone()))
            .await
            .internal_err()?
            .into_iter()
            .map(|t| t.name.to_string())
            .collect();
        Ok(DiscoveryInstallResponse {
            kind: "server".to_string(),
            message: format!("{ext_name} is now active in this session"),
            extension_name: Some(ext_name),
            tools,
        })
    }

    pub(super) async fn on_discovery_dismiss(
        &self,
        req: DiscoveryDismissRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        let agent = self.get_session_agent(&req.session_id).await?;
        agent.discovery.mark_dismissed(&req.id);
        Ok(EmptyResponse {})
    }
}
