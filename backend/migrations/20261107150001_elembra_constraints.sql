--
-- PostgreSQL database dump
--


-- Dumped from database version 16.15 (Debian 16.15-1.pgdg12+2)
-- Dumped by pg_dump version 16.15 (Debian 16.15-1.pgdg12+2)

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

SET default_tablespace = '';

--
-- Name: admin_actions admin_actions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.admin_actions
    ADD CONSTRAINT admin_actions_pkey PRIMARY KEY (id);


--
-- Name: application_enablements application_enablements_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.application_enablements
    ADD CONSTRAINT application_enablements_pkey PRIMARY KEY (tenant_id, workspace_id, application_id);


--
-- Name: application_user_preferences application_user_preferences_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.application_user_preferences
    ADD CONSTRAINT application_user_preferences_pkey PRIMARY KEY (user_id, application_id);


--
-- Name: calendar_event_meeting_legacy_scans calendar_event_meeting_legacy_scans_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_event_meeting_legacy_scans
    ADD CONSTRAINT calendar_event_meeting_legacy_scans_pkey PRIMARY KEY (tenant_id, owner_id);


--
-- Name: calendar_event_meeting_links calendar_event_meeting_links_created_event_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_event_meeting_links
    ADD CONSTRAINT calendar_event_meeting_links_created_event_id_key UNIQUE (created_event_id);


--
-- Name: calendar_event_meeting_links calendar_event_meeting_links_meeting_folder_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_event_meeting_links
    ADD CONSTRAINT calendar_event_meeting_links_meeting_folder_id_key UNIQUE (meeting_folder_id);


--
-- Name: calendar_event_meeting_links calendar_event_meeting_links_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_event_meeting_links
    ADD CONSTRAINT calendar_event_meeting_links_pkey PRIMARY KEY (tenant_id, owner_id, event_id);


--
-- Name: calendar_events calendar_events_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_events
    ADD CONSTRAINT calendar_events_pkey PRIMARY KEY (id);


--
-- Name: calendar_import_jobs calendar_import_jobs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_import_jobs
    ADD CONSTRAINT calendar_import_jobs_pkey PRIMARY KEY (id);


--
-- Name: calendar_mail_links calendar_mail_links_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_mail_links
    ADD CONSTRAINT calendar_mail_links_pkey PRIMARY KEY (id);


--
-- Name: calendar_oauth_states calendar_oauth_states_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_oauth_states
    ADD CONSTRAINT calendar_oauth_states_pkey PRIMARY KEY (state);


--
-- Name: calendar_sources calendar_sources_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_sources
    ADD CONSTRAINT calendar_sources_pkey PRIMARY KEY (id);


--
-- Name: calendar_sync_states calendar_sync_states_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_sync_states
    ADD CONSTRAINT calendar_sync_states_pkey PRIMARY KEY (source_id);


--
-- Name: chat_binding_challenges chat_binding_challenges_nonce_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_binding_challenges
    ADD CONSTRAINT chat_binding_challenges_nonce_key UNIQUE (nonce);


--
-- Name: chat_binding_challenges chat_binding_challenges_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_binding_challenges
    ADD CONSTRAINT chat_binding_challenges_pkey PRIMARY KEY (challenge_id);


--
-- Name: chat_buzz_admissions chat_buzz_admissions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_buzz_admissions
    ADD CONSTRAINT chat_buzz_admissions_pkey PRIMARY KEY (admission_id);


--
-- Name: chat_buzz_admissions chat_buzz_admissions_tenant_id_admission_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_buzz_admissions
    ADD CONSTRAINT chat_buzz_admissions_tenant_id_admission_id_key UNIQUE (tenant_id, admission_id);


--
-- Name: chat_buzz_membership_command_attempts chat_buzz_membership_command_attempts_command_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_buzz_membership_command_attempts
    ADD CONSTRAINT chat_buzz_membership_command_attempts_command_id_key UNIQUE (command_id);


--
-- Name: chat_buzz_membership_command_attempts chat_buzz_membership_command_attempts_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_buzz_membership_command_attempts
    ADD CONSTRAINT chat_buzz_membership_command_attempts_pkey PRIMARY KEY (source, event_id, attempt);


--
-- Name: chat_buzz_membership_operations chat_buzz_membership_operations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_buzz_membership_operations
    ADD CONSTRAINT chat_buzz_membership_operations_pkey PRIMARY KEY (source, event_id);


--
-- Name: chat_buzz_membership_verifications chat_buzz_membership_verifications_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_buzz_membership_verifications
    ADD CONSTRAINT chat_buzz_membership_verifications_pkey PRIMARY KEY (request_id);


--
-- Name: chat_identity_bindings chat_identity_bindings_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_identity_bindings
    ADD CONSTRAINT chat_identity_bindings_pkey PRIMARY KEY (binding_id);


--
-- Name: chat_identity_bindings chat_identity_bindings_tenant_id_binding_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_identity_bindings
    ADD CONSTRAINT chat_identity_bindings_tenant_id_binding_id_key UNIQUE (tenant_id, binding_id);


--
-- Name: chat_identity_replacement_audit chat_identity_replacement_audit_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_identity_replacement_audit
    ADD CONSTRAINT chat_identity_replacement_audit_pkey PRIMARY KEY (audit_id);


--
-- Name: chat_identity_replacement_commands chat_identity_replacement_comman_operation_id_phase_attempt_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_identity_replacement_commands
    ADD CONSTRAINT chat_identity_replacement_comman_operation_id_phase_attempt_key UNIQUE (operation_id, phase, attempt);


--
-- Name: chat_identity_replacement_commands chat_identity_replacement_commands_outbox_event_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_identity_replacement_commands
    ADD CONSTRAINT chat_identity_replacement_commands_outbox_event_id_key UNIQUE (outbox_event_id);


--
-- Name: chat_identity_replacement_commands chat_identity_replacement_commands_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_identity_replacement_commands
    ADD CONSTRAINT chat_identity_replacement_commands_pkey PRIMARY KEY (command_id);


--
-- Name: chat_identity_replacements chat_identity_replacements_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_identity_replacements
    ADD CONSTRAINT chat_identity_replacements_pkey PRIMARY KEY (operation_id);


--
-- Name: chat_identity_replacements chat_identity_replacements_tenant_id_operation_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_identity_replacements
    ADD CONSTRAINT chat_identity_replacements_tenant_id_operation_id_key UNIQUE (tenant_id, operation_id);


--
-- Name: chat_observed_events chat_observed_events_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_observed_events
    ADD CONSTRAINT chat_observed_events_pkey PRIMARY KEY (tenant_id, event_id);


--
-- Name: chat_workspace_communities chat_workspace_communities_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_workspace_communities
    ADD CONSTRAINT chat_workspace_communities_pkey PRIMARY KEY (mapping_id);


--
-- Name: chat_workspace_communities chat_workspace_communities_tenant_id_community_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_workspace_communities
    ADD CONSTRAINT chat_workspace_communities_tenant_id_community_id_key UNIQUE (tenant_id, community_id);


--
-- Name: chat_workspace_communities chat_workspace_communities_tenant_id_mapping_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_workspace_communities
    ADD CONSTRAINT chat_workspace_communities_tenant_id_mapping_id_key UNIQUE (tenant_id, mapping_id);


--
-- Name: chat_workspace_communities chat_workspace_communities_tenant_id_workspace_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_workspace_communities
    ADD CONSTRAINT chat_workspace_communities_tenant_id_workspace_id_key UNIQUE (tenant_id, workspace_id);


--
-- Name: device_pair_requests device_pair_requests_device_code_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.device_pair_requests
    ADD CONSTRAINT device_pair_requests_device_code_key UNIQUE (device_code);


--
-- Name: device_pair_requests device_pair_requests_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.device_pair_requests
    ADD CONSTRAINT device_pair_requests_pkey PRIMARY KEY (id);


--
-- Name: device_pair_requests device_pair_requests_user_code_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.device_pair_requests
    ADD CONSTRAINT device_pair_requests_user_code_key UNIQUE (user_code);


--
-- Name: device_tokens device_tokens_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.device_tokens
    ADD CONSTRAINT device_tokens_pkey PRIMARY KEY (id);


--
-- Name: device_tokens device_tokens_token_hash_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.device_tokens
    ADD CONSTRAINT device_tokens_token_hash_key UNIQUE (token_hash);


--
-- Name: events events_event_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.events
    ADD CONSTRAINT events_event_id_key UNIQUE (event_id);


--
-- Name: events events_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.events
    ADD CONSTRAINT events_pkey PRIMARY KEY (id);


--
-- Name: file_thumbnails file_thumbnails_file_id_size_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.file_thumbnails
    ADD CONSTRAINT file_thumbnails_file_id_size_key UNIQUE (file_id, size);


--
-- Name: file_thumbnails file_thumbnails_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.file_thumbnails
    ADD CONSTRAINT file_thumbnails_pkey PRIMARY KEY (id);


--
-- Name: file_versions file_versions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.file_versions
    ADD CONSTRAINT file_versions_pkey PRIMARY KEY (id);


--
-- Name: files files_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.files
    ADD CONSTRAINT files_pkey PRIMARY KEY (id);


--
-- Name: folders folders_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.folders
    ADD CONSTRAINT folders_pkey PRIMARY KEY (id);


--
-- Name: group_members group_members_group_id_user_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.group_members
    ADD CONSTRAINT group_members_group_id_user_id_key UNIQUE (group_id, user_id);


--
-- Name: group_members group_members_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.group_members
    ADD CONSTRAINT group_members_pkey PRIMARY KEY (id);


--
-- Name: integration_consumer_receipts integration_consumer_receipts_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.integration_consumer_receipts
    ADD CONSTRAINT integration_consumer_receipts_pkey PRIMARY KEY (consumer_id, source, event_id);


--
-- Name: integration_consumer_subscriptions integration_consumer_subscriptions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.integration_consumer_subscriptions
    ADD CONSTRAINT integration_consumer_subscriptions_pkey PRIMARY KEY (consumer_id, pattern);


--
-- Name: integration_consumers integration_consumers_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.integration_consumers
    ADD CONSTRAINT integration_consumers_pkey PRIMARY KEY (consumer_id);


--
-- Name: integration_deliveries integration_deliveries_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.integration_deliveries
    ADD CONSTRAINT integration_deliveries_pkey PRIMARY KEY (consumer_id, source, event_id);


--
-- Name: integration_outbox integration_outbox_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.integration_outbox
    ADD CONSTRAINT integration_outbox_pkey PRIMARY KEY (source, event_id);


--
-- Name: invite_token_group_assignments invite_token_group_assignments_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.invite_token_group_assignments
    ADD CONSTRAINT invite_token_group_assignments_pkey PRIMARY KEY (invite_token_id, group_id);


--
-- Name: invite_tokens invite_tokens_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.invite_tokens
    ADD CONSTRAINT invite_tokens_pkey PRIMARY KEY (id);


--
-- Name: invite_tokens invite_tokens_tenant_id_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.invite_tokens
    ADD CONSTRAINT invite_tokens_tenant_id_id_key UNIQUE (tenant_id, id);


--
-- Name: invite_tokens invite_tokens_tenant_id_token_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.invite_tokens
    ADD CONSTRAINT invite_tokens_tenant_id_token_key UNIQUE (tenant_id, token);


--
-- Name: login_attempts login_attempts_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.login_attempts
    ADD CONSTRAINT login_attempts_pkey PRIMARY KEY (ip_address);


--
-- Name: mail_accounts mail_accounts_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.mail_accounts
    ADD CONSTRAINT mail_accounts_pkey PRIMARY KEY (id);


--
-- Name: mail_attachments mail_attachments_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.mail_attachments
    ADD CONSTRAINT mail_attachments_pkey PRIMARY KEY (id);


--
-- Name: mail_import_jobs mail_import_jobs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.mail_import_jobs
    ADD CONSTRAINT mail_import_jobs_pkey PRIMARY KEY (id);


--
-- Name: mail_links mail_links_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.mail_links
    ADD CONSTRAINT mail_links_pkey PRIMARY KEY (id);


--
-- Name: mail_message_parts mail_message_parts_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.mail_message_parts
    ADD CONSTRAINT mail_message_parts_pkey PRIMARY KEY (id);


--
-- Name: mail_messages mail_messages_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.mail_messages
    ADD CONSTRAINT mail_messages_pkey PRIMARY KEY (id);


--
-- Name: mail_send_idempotency mail_send_idempotency_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.mail_send_idempotency
    ADD CONSTRAINT mail_send_idempotency_pkey PRIMARY KEY (tenant_id, owner_id, account_id, idempotency_key);


--
-- Name: mail_smtp_settings mail_smtp_settings_mail_account_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.mail_smtp_settings
    ADD CONSTRAINT mail_smtp_settings_mail_account_id_key UNIQUE (mail_account_id);


--
-- Name: mail_smtp_settings mail_smtp_settings_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.mail_smtp_settings
    ADD CONSTRAINT mail_smtp_settings_pkey PRIMARY KEY (id);


--
-- Name: managed_chat_identities managed_chat_identities_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.managed_chat_identities
    ADD CONSTRAINT managed_chat_identities_pkey PRIMARY KEY (identity_id);


--
-- Name: managed_chat_identities managed_chat_identities_public_key_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.managed_chat_identities
    ADD CONSTRAINT managed_chat_identities_public_key_key UNIQUE (public_key);


--
-- Name: managed_chat_identities managed_chat_identities_tenant_id_identity_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.managed_chat_identities
    ADD CONSTRAINT managed_chat_identities_tenant_id_identity_id_key UNIQUE (tenant_id, identity_id);


--
-- Name: managed_chat_identities managed_chat_identities_tenant_id_principal_id_generation_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.managed_chat_identities
    ADD CONSTRAINT managed_chat_identities_tenant_id_principal_id_generation_key UNIQUE (tenant_id, principal_id, generation);


--
-- Name: managed_chat_identities managed_chat_identities_tenant_id_principal_id_idempotency__key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.managed_chat_identities
    ADD CONSTRAINT managed_chat_identities_tenant_id_principal_id_idempotency__key UNIQUE (tenant_id, principal_id, idempotency_key);


--
-- Name: managed_chat_identity_operations managed_chat_identity_operati_tenant_id_principal_id_idempo_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.managed_chat_identity_operations
    ADD CONSTRAINT managed_chat_identity_operati_tenant_id_principal_id_idempo_key UNIQUE (tenant_id, principal_id, idempotency_key);


--
-- Name: managed_chat_identity_operations managed_chat_identity_operations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.managed_chat_identity_operations
    ADD CONSTRAINT managed_chat_identity_operations_pkey PRIMARY KEY (operation_id);


--
-- Name: meeting_action_commands meeting_action_commands_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_action_commands
    ADD CONSTRAINT meeting_action_commands_pkey PRIMARY KEY (tenant_id, actor_id, request_id);


--
-- Name: meeting_actions meeting_actions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_actions
    ADD CONSTRAINT meeting_actions_pkey PRIMARY KEY (id);


--
-- Name: meeting_decision_creations meeting_decision_creations_decision_file_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_decision_creations
    ADD CONSTRAINT meeting_decision_creations_decision_file_id_key UNIQUE (decision_file_id);


--
-- Name: meeting_decision_creations meeting_decision_creations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_decision_creations
    ADD CONSTRAINT meeting_decision_creations_pkey PRIMARY KEY (tenant_id, actor_id, request_id);


--
-- Name: meeting_decision_creations meeting_decision_creations_sidecar_file_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_decision_creations
    ADD CONSTRAINT meeting_decision_creations_sidecar_file_id_key UNIQUE (sidecar_file_id);


--
-- Name: meeting_resource_links meeting_resource_links_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_resource_links
    ADD CONSTRAINT meeting_resource_links_pkey PRIMARY KEY (id);


--
-- Name: meeting_resource_links meeting_resource_links_unique_ref; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_resource_links
    ADD CONSTRAINT meeting_resource_links_unique_ref UNIQUE (tenant_id, meeting_folder_id, resource_ref_uri);


--
-- Name: memory_catalog memory_catalog_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.memory_catalog
    ADD CONSTRAINT memory_catalog_pkey PRIMARY KEY (record_id);


--
-- Name: memory_catalog memory_catalog_tenant_id_source_application_source_type_mes_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.memory_catalog
    ADD CONSTRAINT memory_catalog_tenant_id_source_application_source_type_mes_key UNIQUE (tenant_id, source_application, source_type, message_id);


--
-- Name: memory_files_reconcile_discovery memory_files_reconcile_discovery_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.memory_files_reconcile_discovery
    ADD CONSTRAINT memory_files_reconcile_discovery_pkey PRIMARY KEY (singleton);


--
-- Name: memory_files_reconcile_jobs memory_files_reconcile_jobs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.memory_files_reconcile_jobs
    ADD CONSTRAINT memory_files_reconcile_jobs_pkey PRIMARY KEY (tenant_id);


--
-- Name: memory_note_orphan_cleanup_discovery memory_note_orphan_cleanup_discovery_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.memory_note_orphan_cleanup_discovery
    ADD CONSTRAINT memory_note_orphan_cleanup_discovery_pkey PRIMARY KEY (singleton);


--
-- Name: memory_note_orphan_cleanup_jobs memory_note_orphan_cleanup_jobs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.memory_note_orphan_cleanup_jobs
    ADD CONSTRAINT memory_note_orphan_cleanup_jobs_pkey PRIMARY KEY (tenant_id);


--
-- Name: note_index_chunks note_index_chunks_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.note_index_chunks
    ADD CONSTRAINT note_index_chunks_pkey PRIMARY KEY (id);


--
-- Name: notifications notifications_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.notifications
    ADD CONSTRAINT notifications_pkey PRIMARY KEY (id);


--
-- Name: object_gc_queue object_gc_queue_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.object_gc_queue
    ADD CONSTRAINT object_gc_queue_pkey PRIMARY KEY (object_key);


--
-- Name: oidc_config oidc_config_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oidc_config
    ADD CONSTRAINT oidc_config_pkey PRIMARY KEY (id);


--
-- Name: oidc_login_states oidc_login_states_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oidc_login_states
    ADD CONSTRAINT oidc_login_states_pkey PRIMARY KEY (state);


--
-- Name: organization_administrators organization_administrators_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.organization_administrators
    ADD CONSTRAINT organization_administrators_pkey PRIMARY KEY (id);


--
-- Name: organization_invite_resend_attempts organization_invite_resend_at_tenant_id_invite_token_id_ide_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.organization_invite_resend_attempts
    ADD CONSTRAINT organization_invite_resend_at_tenant_id_invite_token_id_ide_key UNIQUE (tenant_id, invite_token_id, idempotency_key_hash);


--
-- Name: organization_invite_resend_attempts organization_invite_resend_attempts_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.organization_invite_resend_attempts
    ADD CONSTRAINT organization_invite_resend_attempts_pkey PRIMARY KEY (id);


--
-- Name: replication_attempts replication_attempts_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.replication_attempts
    ADD CONSTRAINT replication_attempts_pkey PRIMARY KEY (id);


--
-- Name: replication_jobs replication_jobs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.replication_jobs
    ADD CONSTRAINT replication_jobs_pkey PRIMARY KEY (id);


--
-- Name: replication_targets replication_targets_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.replication_targets
    ADD CONSTRAINT replication_targets_pkey PRIMARY KEY (id);


--
-- Name: security_config security_config_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.security_config
    ADD CONSTRAINT security_config_pkey PRIMARY KEY (id);


--
-- Name: share_access_log share_access_log_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.share_access_log
    ADD CONSTRAINT share_access_log_pkey PRIMARY KEY (id);


--
-- Name: share_access_notifications share_access_notifications_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.share_access_notifications
    ADD CONSTRAINT share_access_notifications_pkey PRIMARY KEY (id);


--
-- Name: share_access_notifications share_access_notifications_user_id_share_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.share_access_notifications
    ADD CONSTRAINT share_access_notifications_user_id_share_id_key UNIQUE (user_id, share_id);


--
-- Name: shares shares_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.shares
    ADD CONSTRAINT shares_pkey PRIMARY KEY (id);


--
-- Name: smtp_config smtp_config_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.smtp_config
    ADD CONSTRAINT smtp_config_pkey PRIMARY KEY (id);


--
-- Name: templates templates_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.templates
    ADD CONSTRAINT templates_pkey PRIMARY KEY (id);


--
-- Name: templates templates_template_key_tenant_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.templates
    ADD CONSTRAINT templates_template_key_tenant_id_key UNIQUE (template_key, tenant_id);


--
-- Name: tenants tenants_domain_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tenants
    ADD CONSTRAINT tenants_domain_key UNIQUE (domain);


--
-- Name: tenants tenants_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tenants
    ADD CONSTRAINT tenants_pkey PRIMARY KEY (id);


--
-- Name: upstream_buzz_mapping_operations upstream_buzz_mapping_operations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.upstream_buzz_mapping_operations
    ADD CONSTRAINT upstream_buzz_mapping_operations_pkey PRIMARY KEY (operation_id);


--
-- Name: upstream_buzz_relay_mapping_audit upstream_buzz_relay_mapping_audit_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.upstream_buzz_relay_mapping_audit
    ADD CONSTRAINT upstream_buzz_relay_mapping_audit_pkey PRIMARY KEY (audit_entry_id);


--
-- Name: upstream_buzz_tenant_relays upstream_buzz_tenant_relays_audit_entry_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.upstream_buzz_tenant_relays
    ADD CONSTRAINT upstream_buzz_tenant_relays_audit_entry_id_key UNIQUE (audit_entry_id);


--
-- Name: upstream_buzz_tenant_relays upstream_buzz_tenant_relays_community_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.upstream_buzz_tenant_relays
    ADD CONSTRAINT upstream_buzz_tenant_relays_community_id_key UNIQUE (community_id);


--
-- Name: upstream_buzz_tenant_relays upstream_buzz_tenant_relays_endpoint_service_identity_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.upstream_buzz_tenant_relays
    ADD CONSTRAINT upstream_buzz_tenant_relays_endpoint_service_identity_key UNIQUE (endpoint_service_identity);


--
-- Name: upstream_buzz_tenant_relays upstream_buzz_tenant_relays_mapping_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.upstream_buzz_tenant_relays
    ADD CONSTRAINT upstream_buzz_tenant_relays_mapping_id_key UNIQUE (mapping_id);


--
-- Name: upstream_buzz_tenant_relays upstream_buzz_tenant_relays_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.upstream_buzz_tenant_relays
    ADD CONSTRAINT upstream_buzz_tenant_relays_pkey PRIMARY KEY (tenant_id, mapping_id);


--
-- Name: upstream_buzz_tenant_relays upstream_buzz_tenant_relays_relay_deployment_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.upstream_buzz_tenant_relays
    ADD CONSTRAINT upstream_buzz_tenant_relays_relay_deployment_id_key UNIQUE (relay_deployment_id);


--
-- Name: upstream_buzz_tenant_relays upstream_buzz_tenant_relays_relay_host_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.upstream_buzz_tenant_relays
    ADD CONSTRAINT upstream_buzz_tenant_relays_relay_host_key UNIQUE (relay_host);


--
-- Name: upstream_buzz_tenant_relays upstream_buzz_tenant_relays_relay_pubkey_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.upstream_buzz_tenant_relays
    ADD CONSTRAINT upstream_buzz_tenant_relays_relay_pubkey_key UNIQUE (relay_pubkey);


--
-- Name: upstream_buzz_tenant_relays upstream_buzz_tenant_relays_relay_url_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.upstream_buzz_tenant_relays
    ADD CONSTRAINT upstream_buzz_tenant_relays_relay_url_key UNIQUE (relay_url);


--
-- Name: upstream_buzz_tenant_relays upstream_buzz_tenant_relays_tenant_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.upstream_buzz_tenant_relays
    ADD CONSTRAINT upstream_buzz_tenant_relays_tenant_id_key UNIQUE (tenant_id);


--
-- Name: upstream_buzz_workspace_relay_bindings upstream_buzz_workspace_relay_bindings_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.upstream_buzz_workspace_relay_bindings
    ADD CONSTRAINT upstream_buzz_workspace_relay_bindings_pkey PRIMARY KEY (tenant_id, workspace_id);


--
-- Name: upstream_buzz_workspace_relay_bindings upstream_buzz_workspace_relay_bindings_workspace_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.upstream_buzz_workspace_relay_bindings
    ADD CONSTRAINT upstream_buzz_workspace_relay_bindings_workspace_id_key UNIQUE (workspace_id);


--
-- Name: user_groups user_groups_external_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_groups
    ADD CONSTRAINT user_groups_external_id_key UNIQUE (external_id);


--
-- Name: user_groups user_groups_name_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_groups
    ADD CONSTRAINT user_groups_name_key UNIQUE (name);


--
-- Name: user_groups user_groups_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_groups
    ADD CONSTRAINT user_groups_pkey PRIMARY KEY (id);


--
-- Name: user_security_events user_security_events_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_security_events
    ADD CONSTRAINT user_security_events_pkey PRIMARY KEY (id);


--
-- Name: user_sessions user_sessions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_sessions
    ADD CONSTRAINT user_sessions_pkey PRIMARY KEY (id);


--
-- Name: user_sessions user_sessions_session_token_hash_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_sessions
    ADD CONSTRAINT user_sessions_session_token_hash_key UNIQUE (session_token_hash);


--
-- Name: users users_external_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_external_id_key UNIQUE (external_id);


--
-- Name: users users_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_pkey PRIMARY KEY (id);


--
-- Name: users users_username_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_username_key UNIQUE (username);


--
-- Name: vault_devices vault_devices_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.vault_devices
    ADD CONSTRAINT vault_devices_pkey PRIMARY KEY (id);


--
-- Name: vault_files vault_files_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.vault_files
    ADD CONSTRAINT vault_files_pkey PRIMARY KEY (id);


--
-- Name: vaults vaults_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.vaults
    ADD CONSTRAINT vaults_pkey PRIMARY KEY (id);


--
-- Name: webhook_configs webhook_configs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.webhook_configs
    ADD CONSTRAINT webhook_configs_pkey PRIMARY KEY (id);


--
-- Name: webhook_logs webhook_logs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.webhook_logs
    ADD CONSTRAINT webhook_logs_pkey PRIMARY KEY (id);


--
-- Name: workflows workflows_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.workflows
    ADD CONSTRAINT workflows_pkey PRIMARY KEY (id);


--
-- Name: workflows workflows_tenant_id_key_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.workflows
    ADD CONSTRAINT workflows_tenant_id_key_key UNIQUE (tenant_id, key);


--
-- Name: application_enablements_workspace_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX application_enablements_workspace_idx ON public.application_enablements USING btree (tenant_id, workspace_id, enabled);


--
-- Name: calendar_event_meeting_legacy_scans_expired_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX calendar_event_meeting_legacy_scans_expired_idx ON public.calendar_event_meeting_legacy_scans USING btree (lease_expires_at) WHERE (completed_at IS NULL);


--
-- Name: calendar_event_meeting_links_expired_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX calendar_event_meeting_links_expired_idx ON public.calendar_event_meeting_links USING btree (lease_expires_at) WHERE (state = 'creating'::text);


--
-- Name: calendar_events_external_uid_uidx; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX calendar_events_external_uid_uidx ON public.calendar_events USING btree (source_id, external_uid, COALESCE(recurrence_id, ''::text)) WHERE (deleted_at IS NULL);


--
-- Name: calendar_events_owner_starts_at_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX calendar_events_owner_starts_at_idx ON public.calendar_events USING btree (owner_id, starts_at) WHERE (deleted_at IS NULL);


--
-- Name: calendar_events_recurring_owner_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX calendar_events_recurring_owner_idx ON public.calendar_events USING btree (owner_id) WHERE ((rrule IS NOT NULL) AND (deleted_at IS NULL));


--
-- Name: calendar_import_jobs_owner_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX calendar_import_jobs_owner_id ON public.calendar_import_jobs USING btree (owner_id);


--
-- Name: calendar_import_jobs_source_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX calendar_import_jobs_source_id ON public.calendar_import_jobs USING btree (source_id);


--
-- Name: calendar_import_jobs_status; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX calendar_import_jobs_status ON public.calendar_import_jobs USING btree (status) WHERE (deleted_at IS NULL);


--
-- Name: calendar_import_jobs_tenant_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX calendar_import_jobs_tenant_id ON public.calendar_import_jobs USING btree (tenant_id);


--
-- Name: calendar_mail_links_active_event_uidx; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX calendar_mail_links_active_event_uidx ON public.calendar_mail_links USING btree (tenant_id, owner_id, event_id) WHERE (deleted_at IS NULL);


--
-- Name: calendar_mail_links_active_ref_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX calendar_mail_links_active_ref_idx ON public.calendar_mail_links USING btree (tenant_id, owner_id, mail_message_ref) WHERE (deleted_at IS NULL);


--
-- Name: calendar_oauth_states_expires_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX calendar_oauth_states_expires_idx ON public.calendar_oauth_states USING btree (expires_at);


--
-- Name: calendar_oauth_states_owner_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX calendar_oauth_states_owner_idx ON public.calendar_oauth_states USING btree (owner_id);


--
-- Name: calendar_sources_external_uidx; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX calendar_sources_external_uidx ON public.calendar_sources USING btree (owner_id, kind, external_account, external_calendar_id) WHERE (((kind)::text <> 'internal'::text) AND (deleted_at IS NULL));


--
-- Name: calendar_sources_internal_uidx; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX calendar_sources_internal_uidx ON public.calendar_sources USING btree (tenant_id, owner_id) WHERE (((kind)::text = 'internal'::text) AND (deleted_at IS NULL));


--
-- Name: calendar_sources_owner_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX calendar_sources_owner_idx ON public.calendar_sources USING btree (owner_id) WHERE (deleted_at IS NULL);


--
-- Name: chat_binding_challenges_expiry; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX chat_binding_challenges_expiry ON public.chat_binding_challenges USING btree (expires_at);


--
-- Name: chat_buzz_admissions_live; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX chat_buzz_admissions_live ON public.chat_buzz_admissions USING btree (tenant_id, mapping_id, buzz_pubkey) WHERE active;


--
-- Name: chat_buzz_admissions_lookup; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX chat_buzz_admissions_lookup ON public.chat_buzz_admissions USING btree (tenant_id, mapping_id, buzz_pubkey) WHERE active;


--
-- Name: chat_buzz_membership_accepted_revision; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX chat_buzz_membership_accepted_revision ON public.chat_buzz_membership_verifications USING btree (community_id, pubkey, accepted_revision DESC) WHERE (accepted_at IS NOT NULL);


--
-- Name: chat_buzz_membership_operation; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX chat_buzz_membership_operation ON public.chat_buzz_membership_verifications USING btree (tenant_id, operation_id, requested_at) WHERE (operation_id IS NOT NULL);


--
-- Name: chat_identity_bindings_live_key; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX chat_identity_bindings_live_key ON public.chat_identity_bindings USING btree (tenant_id, buzz_pubkey) WHERE (revoked_at IS NULL);


--
-- Name: chat_identity_bindings_one_active_principal; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX chat_identity_bindings_one_active_principal ON public.chat_identity_bindings USING btree (tenant_id, principal_id) WHERE ((status = 'active'::text) AND (revoked_at IS NULL));


--
-- Name: chat_identity_replacement_audit_tenant; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX chat_identity_replacement_audit_tenant ON public.chat_identity_replacement_audit USING btree (tenant_id, operation_id, created_at, audit_id);


--
-- Name: chat_identity_replacements_one_live_principal; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX chat_identity_replacements_one_live_principal ON public.chat_identity_replacements USING btree (tenant_id, principal_id) WHERE (state <> 'completed'::text);


--
-- Name: chat_identity_replacements_work; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX chat_identity_replacements_work ON public.chat_identity_replacements USING btree (retry_at, lease_expires_at, created_at) WHERE ((state <> 'completed'::text) AND (blocked_at IS NULL));


--
-- Name: chat_observed_events_community; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX chat_observed_events_community ON public.chat_observed_events USING btree (tenant_id, community_id);


--
-- Name: chat_observed_events_message; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX chat_observed_events_message ON public.chat_observed_events USING btree (tenant_id, message_id);


--
-- Name: chat_workspace_communities_active_community; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX chat_workspace_communities_active_community ON public.chat_workspace_communities USING btree (community_id) WHERE active;


--
-- Name: idx_admin_actions_action_type; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_admin_actions_action_type ON public.admin_actions USING btree (action_type);


--
-- Name: idx_admin_actions_actor_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_admin_actions_actor_id ON public.admin_actions USING btree (actor_id);


--
-- Name: idx_admin_actions_performed_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_admin_actions_performed_at ON public.admin_actions USING btree (performed_at DESC);


--
-- Name: idx_admin_actions_target_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_admin_actions_target_id ON public.admin_actions USING btree (target_id) WHERE (target_id IS NOT NULL);


--
-- Name: idx_admin_actions_tenant_performed_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_admin_actions_tenant_performed_id ON public.admin_actions USING btree (tenant_id, performed_at DESC, id DESC);


--
-- Name: idx_deliveries_claimable; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_deliveries_claimable ON public.integration_deliveries USING btree (state, available_at, consumer_id);


--
-- Name: idx_deliveries_consumer_state; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_deliveries_consumer_state ON public.integration_deliveries USING btree (consumer_id, state);


--
-- Name: idx_device_pair_requests_device_code; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_device_pair_requests_device_code ON public.device_pair_requests USING btree (device_code);


--
-- Name: idx_device_pair_requests_expires_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_device_pair_requests_expires_at ON public.device_pair_requests USING btree (expires_at);


--
-- Name: idx_device_pair_requests_user_code; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_device_pair_requests_user_code ON public.device_pair_requests USING btree (user_code);


--
-- Name: idx_device_tokens_token_hash; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_device_tokens_token_hash ON public.device_tokens USING btree (token_hash);


--
-- Name: idx_device_tokens_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_device_tokens_user_id ON public.device_tokens USING btree (user_id);


--
-- Name: idx_events_aggregate; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_events_aggregate ON public.events USING btree (aggregate_id, aggregate_type);


--
-- Name: idx_events_timestamp; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_events_timestamp ON public.events USING btree ("timestamp");


--
-- Name: idx_events_type; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_events_type ON public.events USING btree (event_type);


--
-- Name: idx_events_user; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_events_user ON public.events USING btree (user_id);


--
-- Name: idx_events_user_timestamp_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_events_user_timestamp_id ON public.events USING btree (user_id, "timestamp", id);


--
-- Name: idx_file_thumbnails_file_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_file_thumbnails_file_id ON public.file_thumbnails USING btree (file_id);


--
-- Name: idx_file_thumbnails_tenant_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_file_thumbnails_tenant_id ON public.file_thumbnails USING btree (tenant_id);


--
-- Name: idx_file_versions_created_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_file_versions_created_at ON public.file_versions USING btree (created_at);


--
-- Name: idx_file_versions_file; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_file_versions_file ON public.file_versions USING btree (file_id, version_number DESC);


--
-- Name: idx_file_versions_replication_state; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_file_versions_replication_state ON public.file_versions USING btree (replication_state);


--
-- Name: idx_file_versions_storage_key; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_file_versions_storage_key ON public.file_versions USING btree (storage_key);


--
-- Name: idx_file_versions_tenant_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_file_versions_tenant_id ON public.file_versions USING btree (tenant_id);


--
-- Name: idx_file_versions_unique; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_file_versions_unique ON public.file_versions USING btree (file_id, version_number);


--
-- Name: idx_files_deleted_at_owner; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_files_deleted_at_owner ON public.files USING btree (deleted_at, owner_id) WHERE (deleted_at IS NOT NULL);


--
-- Name: idx_files_hash; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_files_hash ON public.files USING btree (content_hash);


--
-- Name: idx_files_owner; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_files_owner ON public.files USING btree (owner_id);


--
-- Name: idx_files_owner_deleted; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_files_owner_deleted ON public.files USING btree (owner_id, deleted_at) WHERE (deleted_at IS NOT NULL);


--
-- Name: idx_files_owner_path_live_unique; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_files_owner_path_live_unique ON public.files USING btree (owner_id, path) WHERE (deleted_at IS NULL);


--
-- Name: idx_files_owner_starred; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_files_owner_starred ON public.files USING btree (owner_id, starred_at) WHERE (deleted_at IS NULL);


--
-- Name: idx_files_parent; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_files_parent ON public.files USING btree (parent_folder_id);


--
-- Name: idx_files_path; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_files_path ON public.files USING btree (path);


--
-- Name: idx_files_storage_key; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_files_storage_key ON public.files USING btree (storage_key);


--
-- Name: idx_files_tenant_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_files_tenant_id ON public.files USING btree (tenant_id);


--
-- Name: idx_folders_deleted_at_owner; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_folders_deleted_at_owner ON public.folders USING btree (deleted_at, owner_id) WHERE (deleted_at IS NOT NULL);


--
-- Name: idx_folders_owner; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_folders_owner ON public.folders USING btree (owner_id);


--
-- Name: idx_folders_owner_deleted; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_folders_owner_deleted ON public.folders USING btree (owner_id, deleted_at) WHERE (deleted_at IS NOT NULL);


--
-- Name: idx_folders_owner_starred; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_folders_owner_starred ON public.folders USING btree (owner_id, starred_at) WHERE (deleted_at IS NULL);


--
-- Name: idx_folders_parent; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_folders_parent ON public.folders USING btree (parent_folder_id);


--
-- Name: idx_folders_path; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_folders_path ON public.folders USING btree (path);


--
-- Name: idx_folders_tenant_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_folders_tenant_id ON public.folders USING btree (tenant_id);


--
-- Name: idx_folders_unique_name; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_folders_unique_name ON public.folders USING btree (owner_id, parent_folder_id, name) WHERE (deleted_at IS NULL);


--
-- Name: idx_integration_deliveries_event; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_integration_deliveries_event ON public.integration_deliveries USING btree (source, event_id);


--
-- Name: idx_integration_outbox_available; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_integration_outbox_available ON public.integration_outbox USING btree (available_at, created_at);


--
-- Name: idx_integration_outbox_created; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_integration_outbox_created ON public.integration_outbox USING btree (created_at);


--
-- Name: idx_invite_tokens_sender; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_invite_tokens_sender ON public.invite_tokens USING btree (sender_id, created_at);


--
-- Name: idx_invite_tokens_tenant; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_invite_tokens_tenant ON public.invite_tokens USING btree (tenant_id);


--
-- Name: idx_invite_tokens_tenant_created_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_invite_tokens_tenant_created_id ON public.invite_tokens USING btree (tenant_id, created_at DESC, id DESC);


--
-- Name: idx_invite_tokens_token; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_invite_tokens_token ON public.invite_tokens USING btree (token);


--
-- Name: idx_login_attempts_blocked; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_login_attempts_blocked ON public.login_attempts USING btree (blocked_until) WHERE (blocked_until IS NOT NULL);


--
-- Name: idx_mail_accounts_deleted_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_mail_accounts_deleted_at ON public.mail_accounts USING btree (deleted_at);


--
-- Name: idx_mail_accounts_owner_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_mail_accounts_owner_id ON public.mail_accounts USING btree (owner_id);


--
-- Name: idx_mail_accounts_owner_name_unique; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_mail_accounts_owner_name_unique ON public.mail_accounts USING btree (owner_id, name) WHERE (deleted_at IS NULL);


--
-- Name: idx_mail_accounts_tenant_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_mail_accounts_tenant_id ON public.mail_accounts USING btree (tenant_id);


--
-- Name: idx_mail_attachments_blob_key; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_mail_attachments_blob_key ON public.mail_attachments USING btree (blob_key);


--
-- Name: idx_mail_attachments_message_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_mail_attachments_message_id ON public.mail_attachments USING btree (message_id);


--
-- Name: idx_mail_attachments_tenant_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_mail_attachments_tenant_id ON public.mail_attachments USING btree (tenant_id);


--
-- Name: idx_mail_import_jobs_account_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_mail_import_jobs_account_id ON public.mail_import_jobs USING btree (account_id);


--
-- Name: idx_mail_import_jobs_owner_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_mail_import_jobs_owner_id ON public.mail_import_jobs USING btree (owner_id);


--
-- Name: idx_mail_import_jobs_source_mode; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_mail_import_jobs_source_mode ON public.mail_import_jobs USING btree (source_mode) WHERE (deleted_at IS NULL);


--
-- Name: idx_mail_import_jobs_status_created_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_mail_import_jobs_status_created_at ON public.mail_import_jobs USING btree (status, created_at) WHERE (deleted_at IS NULL);


--
-- Name: idx_mail_import_jobs_tenant_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_mail_import_jobs_tenant_id ON public.mail_import_jobs USING btree (tenant_id);


--
-- Name: idx_mail_links_message_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_mail_links_message_id ON public.mail_links USING btree (message_id);


--
-- Name: idx_mail_links_target; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_mail_links_target ON public.mail_links USING btree (target_type, target_id);


--
-- Name: idx_mail_links_tenant_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_mail_links_tenant_id ON public.mail_links USING btree (tenant_id);


--
-- Name: idx_mail_links_unique_active; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_mail_links_unique_active ON public.mail_links USING btree (message_id, target_type, target_id) WHERE (deleted_at IS NULL);


--
-- Name: idx_mail_message_parts_blob_key; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_mail_message_parts_blob_key ON public.mail_message_parts USING btree (blob_key);


--
-- Name: idx_mail_message_parts_message_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_mail_message_parts_message_id ON public.mail_message_parts USING btree (message_id);


--
-- Name: idx_mail_message_parts_tenant_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_mail_message_parts_tenant_id ON public.mail_message_parts USING btree (tenant_id);


--
-- Name: idx_mail_messages_account_source; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_mail_messages_account_source ON public.mail_messages USING btree (owner_id, account_id, source_mode, source_folder, source_uid, source_uidvalidity) NULLS NOT DISTINCT WHERE ((deleted_at IS NULL) AND ((source_mode)::text = ANY ((ARRAY['imap_selected'::character varying, 'imap_archive'::character varying])::text[])));


--
-- Name: idx_mail_messages_archive_job_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_mail_messages_archive_job_id ON public.mail_messages USING btree (archive_job_id) WHERE (deleted_at IS NULL);


--
-- Name: idx_mail_messages_archive_retention; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_mail_messages_archive_retention ON public.mail_messages USING btree (owner_id, account_id, source_folder, archive_job_id, imported_at) WHERE (((source_mode)::text = 'imap_archive'::text) AND (deleted_at IS NULL));


--
-- Name: idx_mail_messages_blob_key; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_mail_messages_blob_key ON public.mail_messages USING btree (blob_key);


--
-- Name: idx_mail_messages_deleted_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_mail_messages_deleted_at ON public.mail_messages USING btree (deleted_at);


--
-- Name: idx_mail_messages_message_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_mail_messages_message_id ON public.mail_messages USING btree (tenant_id, message_id);


--
-- Name: idx_mail_messages_object_key; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_mail_messages_object_key ON public.mail_messages USING btree (object_key);


--
-- Name: idx_mail_messages_owner_drafts; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_mail_messages_owner_drafts ON public.mail_messages USING btree (tenant_id, owner_id, account_id, imported_at DESC, id DESC) WHERE ((deleted_at IS NULL) AND ((source_mode)::text = 'draft'::text));


--
-- Name: idx_mail_messages_owner_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_mail_messages_owner_id ON public.mail_messages USING btree (owner_id);


--
-- Name: idx_mail_messages_owner_imported; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_mail_messages_owner_imported ON public.mail_messages USING btree (tenant_id, owner_id, imported_at DESC, id DESC) WHERE ((deleted_at IS NULL) AND ((source_mode)::text <> 'draft'::text));


--
-- Name: idx_mail_messages_owner_sortdate; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_mail_messages_owner_sortdate ON public.mail_messages USING btree (tenant_id, owner_id, COALESCE(sent_at, imported_at) DESC, id DESC) WHERE ((deleted_at IS NULL) AND ((source_mode)::text <> 'draft'::text));


--
-- Name: idx_mail_messages_sent_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_mail_messages_sent_at ON public.mail_messages USING btree (tenant_id, sent_at DESC);


--
-- Name: idx_mail_messages_tenant_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_mail_messages_tenant_id ON public.mail_messages USING btree (tenant_id);


--
-- Name: idx_mail_smtp_settings_owner_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_mail_smtp_settings_owner_id ON public.mail_smtp_settings USING btree (owner_id);


--
-- Name: idx_mail_smtp_settings_tenant_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_mail_smtp_settings_tenant_id ON public.mail_smtp_settings USING btree (tenant_id);


--
-- Name: idx_note_index_chunks_embedding; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_note_index_chunks_embedding ON public.note_index_chunks USING ivfflat (embedding public.vector_cosine_ops) WITH (lists='100');


--
-- Name: idx_note_index_chunks_tenant_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_note_index_chunks_tenant_id ON public.note_index_chunks USING btree (tenant_id, id);


--
-- Name: idx_note_index_chunks_tenant_note; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_note_index_chunks_tenant_note ON public.note_index_chunks USING btree (tenant_id, note_id);


--
-- Name: idx_note_index_chunks_workspace_note; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_note_index_chunks_workspace_note ON public.note_index_chunks USING btree (workspace_id, note_id);


--
-- Name: idx_notifications_tenant_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_notifications_tenant_id ON public.notifications USING btree (tenant_id);


--
-- Name: idx_object_gc_queue_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_object_gc_queue_id ON public.object_gc_queue USING btree (id);


--
-- Name: idx_object_gc_queue_processing; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_object_gc_queue_processing ON public.object_gc_queue USING btree (locked_at) WHERE ((state)::text = 'processing'::text);


--
-- Name: idx_object_gc_queue_ready; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_object_gc_queue_ready ON public.object_gc_queue USING btree (not_before, created_at) WHERE (((state)::text = ANY ((ARRAY['pending'::character varying, 'retry'::character varying])::text[])) AND (operator_hold = false));


--
-- Name: idx_oidc_login_states_created_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_oidc_login_states_created_at ON public.oidc_login_states USING btree (created_at);


--
-- Name: idx_oidc_login_states_expires_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_oidc_login_states_expires_at ON public.oidc_login_states USING btree (expires_at);


--
-- Name: idx_organization_invite_resend_one_sending; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_organization_invite_resend_one_sending ON public.organization_invite_resend_attempts USING btree (invite_token_id) WHERE (status = 'sending'::text);


--
-- Name: idx_organization_invite_resend_tenant; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_organization_invite_resend_tenant ON public.organization_invite_resend_attempts USING btree (tenant_id, invite_token_id, requested_at DESC);


--
-- Name: idx_replication_attempts_job; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_replication_attempts_job ON public.replication_attempts USING btree (job_id, attempt_number DESC);


--
-- Name: idx_replication_attempts_target; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_replication_attempts_target ON public.replication_attempts USING btree (target_id, completed_at DESC);


--
-- Name: idx_replication_jobs_file_version; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_replication_jobs_file_version ON public.replication_jobs USING btree (file_version_id);


--
-- Name: idx_replication_jobs_status_attempt; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_replication_jobs_status_attempt ON public.replication_jobs USING btree (status, next_attempt_at);


--
-- Name: idx_replication_jobs_storage_key_status; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_replication_jobs_storage_key_status ON public.replication_jobs USING btree (storage_key, status);


--
-- Name: idx_replication_targets_enabled; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_replication_targets_enabled ON public.replication_targets USING btree (enabled);


--
-- Name: idx_resource; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_resource ON public.notifications USING btree (resource_id, resource_type);


--
-- Name: idx_share_access_log_accessed_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_share_access_log_accessed_at ON public.share_access_log USING btree (accessed_at);


--
-- Name: idx_share_access_log_share_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_share_access_log_share_id ON public.share_access_log USING btree (share_id);


--
-- Name: idx_share_access_notifications_share; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_share_access_notifications_share ON public.share_access_notifications USING btree (share_id);


--
-- Name: idx_share_access_notifications_user; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_share_access_notifications_user ON public.share_access_notifications USING btree (user_id);


--
-- Name: idx_shares_active; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_shares_active ON public.shares USING btree (share_token) WHERE (revoked_at IS NULL);


--
-- Name: idx_shares_active_target; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_shares_active_target ON public.shares USING btree (file_id, folder_id, revoked_at) WHERE (revoked_at IS NULL);


--
-- Name: idx_shares_creator; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_shares_creator ON public.shares USING btree (created_by);


--
-- Name: idx_shares_expires_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_shares_expires_at ON public.shares USING btree (expires_at) WHERE (expires_at IS NOT NULL);


--
-- Name: idx_shares_file; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_shares_file ON public.shares USING btree (file_id);


--
-- Name: idx_shares_file_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_shares_file_id ON public.shares USING btree (file_id) WHERE (file_id IS NOT NULL);


--
-- Name: idx_shares_folder; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_shares_folder ON public.shares USING btree (folder_id, revoked_at);


--
-- Name: idx_shares_folder_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_shares_folder_id ON public.shares USING btree (folder_id) WHERE (folder_id IS NOT NULL);


--
-- Name: idx_shares_group_file_unique; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_shares_group_file_unique ON public.shares USING btree (file_id, recipient_group_id) WHERE ((recipient_group_id IS NOT NULL) AND (file_id IS NOT NULL) AND (revoked_at IS NULL));


--
-- Name: idx_shares_group_folder_unique; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_shares_group_folder_unique ON public.shares USING btree (folder_id, recipient_group_id) WHERE ((recipient_group_id IS NOT NULL) AND (folder_id IS NOT NULL) AND (revoked_at IS NULL));


--
-- Name: idx_shares_recipient; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_shares_recipient ON public.shares USING btree (recipient_user_id, revoked_at);


--
-- Name: idx_shares_recipient_group; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_shares_recipient_group ON public.shares USING btree (recipient_group_id, revoked_at) WHERE (recipient_group_id IS NOT NULL);


--
-- Name: idx_shares_tenant_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_shares_tenant_id ON public.shares USING btree (tenant_id);


--
-- Name: idx_shares_token; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_shares_token ON public.shares USING btree (share_token);


--
-- Name: idx_shares_token_unique; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_shares_token_unique ON public.shares USING btree (share_token) WHERE (share_token IS NOT NULL);


--
-- Name: idx_templates_application; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_templates_application ON public.templates USING btree (application_id);


--
-- Name: idx_templates_enabled; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_templates_enabled ON public.templates USING btree (enabled);


--
-- Name: idx_templates_system; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_templates_system ON public.templates USING btree (system_template);


--
-- Name: idx_templates_ui_config; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_templates_ui_config ON public.templates USING gin (ui_config);


--
-- Name: idx_tenants_domain; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_tenants_domain ON public.tenants USING btree (domain);


--
-- Name: idx_user_groups_external_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_user_groups_external_id ON public.user_groups USING btree (external_id);


--
-- Name: idx_user_groups_tenant_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_user_groups_tenant_id ON public.user_groups USING btree (tenant_id);


--
-- Name: idx_user_security_events_user_occurred_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_user_security_events_user_occurred_at ON public.user_security_events USING btree (user_id, occurred_at DESC);


--
-- Name: idx_user_sessions_expires_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_user_sessions_expires_at ON public.user_sessions USING btree (expires_at);


--
-- Name: idx_user_sessions_tenant_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_user_sessions_tenant_id ON public.user_sessions USING btree (tenant_id);


--
-- Name: idx_user_sessions_user; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_user_sessions_user ON public.user_sessions USING btree (user_id);


--
-- Name: idx_user_unread; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_user_unread ON public.notifications USING btree (user_id, read, created_at);


--
-- Name: idx_users_avatar_path; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_users_avatar_path ON public.users USING btree (avatar_path) WHERE (avatar_path IS NOT NULL);


--
-- Name: idx_users_disabled_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_users_disabled_at ON public.users USING btree (disabled_at) WHERE (disabled_at IS NOT NULL);


--
-- Name: idx_users_email; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_users_email ON public.users USING btree (email);


--
-- Name: idx_users_external_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_users_external_id ON public.users USING btree (external_id);


--
-- Name: idx_users_tenant_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_users_tenant_id ON public.users USING btree (tenant_id);


--
-- Name: idx_users_username; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_users_username ON public.users USING btree (username);


--
-- Name: idx_vault_devices_active_webui; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_vault_devices_active_webui ON public.vault_devices USING btree (tenant_id, user_id, vault_id, client_type) WHERE (((client_type)::text = 'web_ui'::text) AND (revoked_at IS NULL));


--
-- Name: idx_vault_devices_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_vault_devices_user_id ON public.vault_devices USING btree (user_id);


--
-- Name: idx_vault_devices_vault_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_vault_devices_vault_id ON public.vault_devices USING btree (vault_id);


--
-- Name: idx_vault_files_sha256; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_vault_files_sha256 ON public.vault_files USING btree (sha256);


--
-- Name: idx_vault_files_unique_path; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_vault_files_unique_path ON public.vault_files USING btree (vault_id, relative_path) WHERE (deleted_at IS NULL);


--
-- Name: idx_vault_files_vault_id_relative_path; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_vault_files_vault_id_relative_path ON public.vault_files USING btree (vault_id, relative_path);


--
-- Name: idx_vault_files_vault_id_server_rev; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_vault_files_vault_id_server_rev ON public.vault_files USING btree (vault_id, server_rev);


--
-- Name: idx_vaults_owner_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_vaults_owner_user_id ON public.vaults USING btree (owner_user_id);


--
-- Name: idx_vaults_tenant_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_vaults_tenant_id ON public.vaults USING btree (tenant_id);


--
-- Name: idx_webhook_logs_created_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_webhook_logs_created_at ON public.webhook_logs USING btree (created_at);


--
-- Name: idx_webhook_logs_webhook_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_webhook_logs_webhook_id ON public.webhook_logs USING btree (webhook_id);


--
-- Name: idx_workflows_key; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_workflows_key ON public.workflows USING btree (key);


--
-- Name: idx_workflows_tenant; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_workflows_tenant ON public.workflows USING btree (tenant_id);


--
-- Name: invite_token_group_assignments_group_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX invite_token_group_assignments_group_id ON public.invite_token_group_assignments USING btree (group_id);


--
-- Name: invite_tokens_expired_recipient_lower_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX invite_tokens_expired_recipient_lower_idx ON public.invite_tokens USING btree (tenant_id, lower((recipient_email)::text), expires_at) WHERE ((used_at IS NULL) AND (revoked_at IS NULL));


--
-- Name: invite_tokens_pending_recipient_lower_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX invite_tokens_pending_recipient_lower_idx ON public.invite_tokens USING btree (lower((recipient_email)::text), created_at DESC) WHERE ((used_at IS NULL) AND (revoked_at IS NULL));


--
-- Name: managed_chat_identities_one_active_principal; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX managed_chat_identities_one_active_principal ON public.managed_chat_identities USING btree (tenant_id, principal_id) WHERE (status = 'active'::text);


--
-- Name: managed_chat_identities_tenant_principal; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX managed_chat_identities_tenant_principal ON public.managed_chat_identities USING btree (tenant_id, principal_id, generation DESC);


--
-- Name: managed_chat_identity_operations_one_active; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX managed_chat_identity_operations_one_active ON public.managed_chat_identity_operations USING btree (tenant_id, principal_id) WHERE (status = ANY (ARRAY['authorized'::text, 'provisioning'::text, 'failed'::text, 'abandoning'::text]));


--
-- Name: meeting_actions_meeting_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX meeting_actions_meeting_idx ON public.meeting_actions USING btree (tenant_id, meeting_id, created_at, id);


--
-- Name: meeting_actions_search_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX meeting_actions_search_idx ON public.meeting_actions USING gin (to_tsvector('simple'::regconfig, ((title || ' '::text) || COALESCE(description, ''::text))));


--
-- Name: meeting_decision_reverse_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX meeting_decision_reverse_idx ON public.meeting_resource_links USING btree (tenant_id, resource_ref_uri, created_at, id) WHERE (link_kind = 'decision'::text);


--
-- Name: meeting_resource_links_order_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX meeting_resource_links_order_idx ON public.meeting_resource_links USING btree (tenant_id, meeting_folder_id, created_at, id);


--
-- Name: memory_catalog_community; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX memory_catalog_community ON public.memory_catalog USING btree (tenant_id, community_id);


--
-- Name: memory_catalog_tenant_status; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX memory_catalog_tenant_status ON public.memory_catalog USING btree (tenant_id, indexing_status);


--
-- Name: memory_files_reconcile_due_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX memory_files_reconcile_due_idx ON public.memory_files_reconcile_jobs USING btree (next_run_at, last_claimed_at, tenant_id) WHERE (status = ANY (ARRAY['idle'::text, 'running'::text]));


--
-- Name: memory_files_reconcile_sources_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX memory_files_reconcile_sources_idx ON public.files USING btree (tenant_id, id);


--
-- Name: memory_note_orphan_cleanup_due_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX memory_note_orphan_cleanup_due_idx ON public.memory_note_orphan_cleanup_jobs USING btree (next_run_at, last_claimed_at, tenant_id) WHERE (status = ANY (ARRAY['idle'::text, 'running'::text]));


--
-- Name: organization_administrators_one_active_grant; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX organization_administrators_one_active_grant ON public.organization_administrators USING btree (tenant_id, user_id) WHERE (revoked_at IS NULL);


--
-- Name: organization_administrators_tenant_granted_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX organization_administrators_tenant_granted_at ON public.organization_administrators USING btree (tenant_id, granted_at DESC, id DESC);


--
-- Name: users_email_tenant_id_key; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX users_email_tenant_id_key ON public.users USING btree (lower((email)::text), tenant_id);


--
-- Name: admin_actions admin_actions_append_only; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER admin_actions_append_only BEFORE UPDATE ON public.admin_actions FOR EACH ROW EXECUTE FUNCTION public.prevent_admin_action_rewrite();


--
-- Name: file_versions file_versions_cancel_blob_gc; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER file_versions_cancel_blob_gc AFTER INSERT OR UPDATE OF storage_key ON public.file_versions FOR EACH ROW EXECUTE FUNCTION public.cancel_object_gc('storage_key');


--
-- Name: file_versions file_versions_queue_blob_gc; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER file_versions_queue_blob_gc AFTER DELETE OR UPDATE OF storage_key ON public.file_versions FOR EACH ROW EXECUTE FUNCTION public.queue_removed_object_key('storage_key');


--
-- Name: files files_cancel_blob_gc; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER files_cancel_blob_gc AFTER INSERT OR UPDATE OF storage_key ON public.files FOR EACH ROW EXECUTE FUNCTION public.cancel_object_gc('storage_key');


--
-- Name: files files_queue_blob_gc; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER files_queue_blob_gc AFTER DELETE OR UPDATE OF storage_key ON public.files FOR EACH ROW EXECUTE FUNCTION public.queue_removed_object_key('storage_key');


--
-- Name: mail_attachments mail_attachments_cancel_blob_gc; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER mail_attachments_cancel_blob_gc AFTER INSERT OR UPDATE OF blob_key ON public.mail_attachments FOR EACH ROW EXECUTE FUNCTION public.cancel_object_gc('blob_key');


--
-- Name: mail_attachments mail_attachments_queue_blob_gc; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER mail_attachments_queue_blob_gc AFTER DELETE OR UPDATE OF blob_key ON public.mail_attachments FOR EACH ROW EXECUTE FUNCTION public.queue_removed_object_key('blob_key');


--
-- Name: mail_messages mail_messages_cancel_blob_gc; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER mail_messages_cancel_blob_gc AFTER INSERT OR UPDATE OF blob_key ON public.mail_messages FOR EACH ROW EXECUTE FUNCTION public.cancel_object_gc('blob_key');


--
-- Name: mail_messages mail_messages_cancel_object_gc; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER mail_messages_cancel_object_gc AFTER INSERT OR UPDATE OF object_key ON public.mail_messages FOR EACH ROW EXECUTE FUNCTION public.cancel_object_gc('object_key');


--
-- Name: mail_messages mail_messages_queue_blob_gc; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER mail_messages_queue_blob_gc AFTER DELETE OR UPDATE OF blob_key ON public.mail_messages FOR EACH ROW EXECUTE FUNCTION public.queue_removed_object_key('blob_key');


--
-- Name: mail_messages mail_messages_queue_object_gc; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER mail_messages_queue_object_gc AFTER DELETE OR UPDATE OF object_key ON public.mail_messages FOR EACH ROW EXECUTE FUNCTION public.queue_removed_object_key('object_key');


--
-- Name: mail_message_parts mail_parts_cancel_blob_gc; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER mail_parts_cancel_blob_gc AFTER INSERT OR UPDATE OF blob_key ON public.mail_message_parts FOR EACH ROW EXECUTE FUNCTION public.cancel_object_gc('blob_key');


--
-- Name: mail_message_parts mail_parts_queue_blob_gc; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER mail_parts_queue_blob_gc AFTER DELETE OR UPDATE OF blob_key ON public.mail_message_parts FOR EACH ROW EXECUTE FUNCTION public.queue_removed_object_key('blob_key');


--
-- Name: managed_chat_identities managed_chat_identities_immutable_attribution; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER managed_chat_identities_immutable_attribution BEFORE DELETE OR UPDATE ON public.managed_chat_identities FOR EACH ROW EXECUTE FUNCTION public.reject_managed_chat_identity_attribution_change();


--
-- Name: object_gc_queue object_gc_queue_write_lock; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER object_gc_queue_write_lock BEFORE INSERT OR DELETE OR UPDATE ON public.object_gc_queue FOR EACH STATEMENT EXECUTE FUNCTION public.serialize_object_gc_queue_writes();


--
-- Name: organization_administrators organization_administrators_append_only; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER organization_administrators_append_only BEFORE DELETE OR UPDATE ON public.organization_administrators FOR EACH ROW EXECUTE FUNCTION public.protect_organization_administrator_history();


--
-- Name: organization_administrators organization_administrators_require_audit; Type: TRIGGER; Schema: public; Owner: -
--

CREATE CONSTRAINT TRIGGER organization_administrators_require_audit AFTER INSERT OR UPDATE ON public.organization_administrators DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION public.require_organization_administrator_audit();


--
-- Name: organization_administrators organization_administrators_same_tenant; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER organization_administrators_same_tenant BEFORE INSERT ON public.organization_administrators FOR EACH ROW EXECUTE FUNCTION public.validate_organization_administrator_grant();


--
-- Name: replication_jobs replication_jobs_cancel_blob_gc; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER replication_jobs_cancel_blob_gc AFTER INSERT OR UPDATE OF storage_key ON public.replication_jobs FOR EACH ROW EXECUTE FUNCTION public.cancel_object_gc('storage_key');


--
-- Name: replication_jobs replication_jobs_queue_blob_gc; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER replication_jobs_queue_blob_gc AFTER DELETE OR UPDATE OF storage_key ON public.replication_jobs FOR EACH ROW EXECUTE FUNCTION public.queue_removed_object_key('storage_key');


--
-- Name: upstream_buzz_tenant_relays upstream_buzz_mapping_attribution_immutable; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER upstream_buzz_mapping_attribution_immutable BEFORE DELETE OR UPDATE ON public.upstream_buzz_tenant_relays FOR EACH ROW EXECUTE FUNCTION public.reject_upstream_buzz_mapping_attribution_change();


--
-- Name: upstream_buzz_relay_mapping_audit upstream_buzz_mapping_audit_append_only; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER upstream_buzz_mapping_audit_append_only BEFORE DELETE OR UPDATE ON public.upstream_buzz_relay_mapping_audit FOR EACH ROW EXECUTE FUNCTION public.reject_upstream_buzz_mapping_audit_change();


--
-- Name: upstream_buzz_relay_mapping_audit upstream_buzz_mapping_audit_operation_matches; Type: TRIGGER; Schema: public; Owner: -
--

CREATE CONSTRAINT TRIGGER upstream_buzz_mapping_audit_operation_matches AFTER INSERT ON public.upstream_buzz_relay_mapping_audit DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION public.verify_upstream_buzz_mapping_audit_operation();


--
-- Name: upstream_buzz_tenant_relays upstream_buzz_mapping_create_audit_matches_mapping; Type: TRIGGER; Schema: public; Owner: -
--

CREATE CONSTRAINT TRIGGER upstream_buzz_mapping_create_audit_matches_mapping AFTER INSERT ON public.upstream_buzz_tenant_relays DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION public.verify_upstream_buzz_mapping_create_audit();


--
-- Name: upstream_buzz_tenant_relays upstream_buzz_mapping_generation_bump; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER upstream_buzz_mapping_generation_bump BEFORE UPDATE ON public.upstream_buzz_tenant_relays FOR EACH ROW EXECUTE FUNCTION public.require_upstream_buzz_mapping_generation_bump();


--
-- Name: upstream_buzz_mapping_operations upstream_buzz_mapping_operations_append_only; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER upstream_buzz_mapping_operations_append_only BEFORE DELETE OR UPDATE ON public.upstream_buzz_mapping_operations FOR EACH ROW EXECUTE FUNCTION public.reject_upstream_buzz_mapping_operation_change();


--
-- Name: upstream_buzz_tenant_relays upstream_buzz_mapping_update_audit_matches_generation; Type: TRIGGER; Schema: public; Owner: -
--

CREATE CONSTRAINT TRIGGER upstream_buzz_mapping_update_audit_matches_generation AFTER UPDATE ON public.upstream_buzz_tenant_relays DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION public.verify_upstream_buzz_mapping_update_audit();


--
-- Name: upstream_buzz_workspace_relay_bindings upstream_buzz_workspace_binding_audit_matches_binding; Type: TRIGGER; Schema: public; Owner: -
--

CREATE CONSTRAINT TRIGGER upstream_buzz_workspace_binding_audit_matches_binding AFTER INSERT ON public.upstream_buzz_workspace_relay_bindings DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION public.verify_upstream_buzz_workspace_binding_audit();


--
-- Name: upstream_buzz_workspace_relay_bindings upstream_buzz_workspace_bindings_append_only; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER upstream_buzz_workspace_bindings_append_only BEFORE DELETE OR UPDATE ON public.upstream_buzz_workspace_relay_bindings FOR EACH ROW EXECUTE FUNCTION public.reject_upstream_buzz_workspace_binding_change();


--
-- Name: users users_delete_revokes_chat; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER users_delete_revokes_chat BEFORE DELETE ON public.users FOR EACH ROW EXECUTE FUNCTION public.revoke_chat_on_user_delete();


--
-- Name: users users_disable_revokes_chat; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER users_disable_revokes_chat AFTER UPDATE OF disabled_at ON public.users FOR EACH ROW EXECUTE FUNCTION public.revoke_chat_on_user_disable();


--
-- Name: users users_disable_revokes_credentials; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER users_disable_revokes_credentials AFTER UPDATE OF disabled_at ON public.users FOR EACH ROW EXECUTE FUNCTION public.revoke_credentials_on_user_disable();


--
-- Name: users users_reserve_pending_invite_identifiers; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER users_reserve_pending_invite_identifiers BEFORE INSERT ON public.users FOR EACH ROW EXECUTE FUNCTION public.prevent_user_identifier_collision_with_pending_invite();


--
-- Name: users users_reserve_pending_invite_identifiers_on_update; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER users_reserve_pending_invite_identifiers_on_update BEFORE UPDATE OF email, username ON public.users FOR EACH ROW EXECUTE FUNCTION public.prevent_user_identifier_collision_with_pending_invite();


--
-- Name: vault_files vault_files_cancel_blob_gc; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER vault_files_cancel_blob_gc AFTER INSERT OR UPDATE OF sha256 ON public.vault_files FOR EACH ROW EXECUTE FUNCTION public.cancel_vault_blob_gc();


--
-- Name: vault_files vault_files_queue_blob_gc; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER vault_files_queue_blob_gc AFTER DELETE OR UPDATE OF sha256 ON public.vault_files FOR EACH ROW EXECUTE FUNCTION public.queue_removed_vault_blob();


--
-- Name: admin_actions admin_actions_actor_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.admin_actions
    ADD CONSTRAINT admin_actions_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: application_user_preferences application_user_preferences_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.application_user_preferences
    ADD CONSTRAINT application_user_preferences_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: calendar_event_meeting_legacy_scans calendar_event_meeting_legacy_scans_owner_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_event_meeting_legacy_scans
    ADD CONSTRAINT calendar_event_meeting_legacy_scans_owner_id_fkey FOREIGN KEY (owner_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: calendar_event_meeting_links calendar_event_meeting_links_owner_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_event_meeting_links
    ADD CONSTRAINT calendar_event_meeting_links_owner_id_fkey FOREIGN KEY (owner_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: calendar_events calendar_events_owner_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_events
    ADD CONSTRAINT calendar_events_owner_id_fkey FOREIGN KEY (owner_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: calendar_events calendar_events_source_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_events
    ADD CONSTRAINT calendar_events_source_id_fkey FOREIGN KEY (source_id) REFERENCES public.calendar_sources(id) ON DELETE CASCADE;


--
-- Name: calendar_import_jobs calendar_import_jobs_owner_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_import_jobs
    ADD CONSTRAINT calendar_import_jobs_owner_id_fkey FOREIGN KEY (owner_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: calendar_import_jobs calendar_import_jobs_source_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_import_jobs
    ADD CONSTRAINT calendar_import_jobs_source_id_fkey FOREIGN KEY (source_id) REFERENCES public.calendar_sources(id) ON DELETE CASCADE;


--
-- Name: calendar_mail_links calendar_mail_links_created_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_mail_links
    ADD CONSTRAINT calendar_mail_links_created_by_fkey FOREIGN KEY (created_by) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: calendar_mail_links calendar_mail_links_event_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_mail_links
    ADD CONSTRAINT calendar_mail_links_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.calendar_events(id) ON DELETE CASCADE;


--
-- Name: calendar_mail_links calendar_mail_links_owner_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_mail_links
    ADD CONSTRAINT calendar_mail_links_owner_id_fkey FOREIGN KEY (owner_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: calendar_oauth_states calendar_oauth_states_owner_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_oauth_states
    ADD CONSTRAINT calendar_oauth_states_owner_id_fkey FOREIGN KEY (owner_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: calendar_sources calendar_sources_owner_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_sources
    ADD CONSTRAINT calendar_sources_owner_id_fkey FOREIGN KEY (owner_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: calendar_sync_states calendar_sync_states_source_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_sync_states
    ADD CONSTRAINT calendar_sync_states_source_id_fkey FOREIGN KEY (source_id) REFERENCES public.calendar_sources(id) ON DELETE CASCADE;


--
-- Name: chat_binding_challenges chat_binding_challenges_rotation_of_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_binding_challenges
    ADD CONSTRAINT chat_binding_challenges_rotation_of_fkey FOREIGN KEY (rotation_of) REFERENCES public.chat_identity_bindings(binding_id);


--
-- Name: chat_buzz_admissions chat_buzz_admissions_tenant_id_binding_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_buzz_admissions
    ADD CONSTRAINT chat_buzz_admissions_tenant_id_binding_id_fkey FOREIGN KEY (tenant_id, binding_id) REFERENCES public.chat_identity_bindings(tenant_id, binding_id);


--
-- Name: chat_buzz_admissions chat_buzz_admissions_tenant_id_mapping_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_buzz_admissions
    ADD CONSTRAINT chat_buzz_admissions_tenant_id_mapping_id_fkey FOREIGN KEY (tenant_id, mapping_id) REFERENCES public.chat_workspace_communities(tenant_id, mapping_id);


--
-- Name: chat_buzz_membership_command_attempts chat_buzz_membership_command_attempts_source_event_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_buzz_membership_command_attempts
    ADD CONSTRAINT chat_buzz_membership_command_attempts_source_event_id_fkey FOREIGN KEY (source, event_id) REFERENCES public.chat_buzz_membership_operations(source, event_id) ON DELETE RESTRICT;


--
-- Name: chat_buzz_membership_operations chat_buzz_membership_operations_tenant_id_mapping_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_buzz_membership_operations
    ADD CONSTRAINT chat_buzz_membership_operations_tenant_id_mapping_id_fkey FOREIGN KEY (tenant_id, mapping_id) REFERENCES public.chat_workspace_communities(tenant_id, mapping_id);


--
-- Name: chat_buzz_membership_verifications chat_buzz_membership_verifications_operation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_buzz_membership_verifications
    ADD CONSTRAINT chat_buzz_membership_verifications_operation_id_fkey FOREIGN KEY (operation_id) REFERENCES public.chat_identity_replacements(operation_id);


--
-- Name: chat_identity_bindings chat_identity_bindings_rotation_of_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_identity_bindings
    ADD CONSTRAINT chat_identity_bindings_rotation_of_fkey FOREIGN KEY (rotation_of) REFERENCES public.chat_identity_bindings(binding_id);


--
-- Name: chat_identity_replacement_audit chat_identity_replacement_audit_operation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_identity_replacement_audit
    ADD CONSTRAINT chat_identity_replacement_audit_operation_id_fkey FOREIGN KEY (operation_id) REFERENCES public.chat_identity_replacements(operation_id);


--
-- Name: chat_identity_replacement_commands chat_identity_replacement_commands_operation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_identity_replacement_commands
    ADD CONSTRAINT chat_identity_replacement_commands_operation_id_fkey FOREIGN KEY (operation_id) REFERENCES public.chat_identity_replacements(operation_id);


--
-- Name: chat_identity_replacements chat_identity_replacements_challenge_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_identity_replacements
    ADD CONSTRAINT chat_identity_replacements_challenge_id_fkey FOREIGN KEY (challenge_id) REFERENCES public.chat_binding_challenges(challenge_id);


--
-- Name: chat_identity_replacements chat_identity_replacements_tenant_id_mapping_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_identity_replacements
    ADD CONSTRAINT chat_identity_replacements_tenant_id_mapping_id_fkey FOREIGN KEY (tenant_id, mapping_id) REFERENCES public.chat_workspace_communities(tenant_id, mapping_id);


--
-- Name: chat_identity_replacements chat_identity_replacements_tenant_id_old_binding_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_identity_replacements
    ADD CONSTRAINT chat_identity_replacements_tenant_id_old_binding_id_fkey FOREIGN KEY (tenant_id, old_binding_id) REFERENCES public.chat_identity_bindings(tenant_id, binding_id);


--
-- Name: chat_identity_replacements chat_identity_replacements_tenant_id_replacement_binding_i_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_identity_replacements
    ADD CONSTRAINT chat_identity_replacements_tenant_id_replacement_binding_i_fkey FOREIGN KEY (tenant_id, replacement_binding_id) REFERENCES public.chat_identity_bindings(tenant_id, binding_id);


--
-- Name: device_pair_requests device_pair_requests_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.device_pair_requests
    ADD CONSTRAINT device_pair_requests_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: device_tokens device_tokens_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.device_tokens
    ADD CONSTRAINT device_tokens_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: file_thumbnails file_thumbnails_file_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.file_thumbnails
    ADD CONSTRAINT file_thumbnails_file_id_fkey FOREIGN KEY (file_id) REFERENCES public.files(id) ON DELETE CASCADE;


--
-- Name: file_versions file_versions_created_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.file_versions
    ADD CONSTRAINT file_versions_created_by_fkey FOREIGN KEY (created_by) REFERENCES public.users(id);


--
-- Name: file_versions file_versions_file_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.file_versions
    ADD CONSTRAINT file_versions_file_id_fkey FOREIGN KEY (file_id) REFERENCES public.files(id) ON DELETE CASCADE;


--
-- Name: files files_owner_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.files
    ADD CONSTRAINT files_owner_id_fkey FOREIGN KEY (owner_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: files files_parent_folder_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.files
    ADD CONSTRAINT files_parent_folder_id_fkey FOREIGN KEY (parent_folder_id) REFERENCES public.folders(id) ON DELETE CASCADE;


--
-- Name: integration_deliveries fk_outbox_event; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.integration_deliveries
    ADD CONSTRAINT fk_outbox_event FOREIGN KEY (source, event_id) REFERENCES public.integration_outbox(source, event_id) ON DELETE CASCADE;


--
-- Name: folders folders_owner_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.folders
    ADD CONSTRAINT folders_owner_id_fkey FOREIGN KEY (owner_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: folders folders_parent_folder_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.folders
    ADD CONSTRAINT folders_parent_folder_id_fkey FOREIGN KEY (parent_folder_id) REFERENCES public.folders(id) ON DELETE CASCADE;


--
-- Name: group_members group_members_added_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.group_members
    ADD CONSTRAINT group_members_added_by_fkey FOREIGN KEY (added_by) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: group_members group_members_group_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.group_members
    ADD CONSTRAINT group_members_group_id_fkey FOREIGN KEY (group_id) REFERENCES public.user_groups(id) ON DELETE CASCADE;


--
-- Name: group_members group_members_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.group_members
    ADD CONSTRAINT group_members_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: integration_consumer_subscriptions integration_consumer_subscriptions_consumer_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.integration_consumer_subscriptions
    ADD CONSTRAINT integration_consumer_subscriptions_consumer_id_fkey FOREIGN KEY (consumer_id) REFERENCES public.integration_consumers(consumer_id) ON DELETE CASCADE;


--
-- Name: invite_token_group_assignments invite_token_group_assignments_invite_token_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.invite_token_group_assignments
    ADD CONSTRAINT invite_token_group_assignments_invite_token_id_fkey FOREIGN KEY (invite_token_id) REFERENCES public.invite_tokens(id) ON DELETE CASCADE;


--
-- Name: invite_tokens invite_tokens_sender_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.invite_tokens
    ADD CONSTRAINT invite_tokens_sender_id_fkey FOREIGN KEY (sender_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: invite_tokens invite_tokens_workflow_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.invite_tokens
    ADD CONSTRAINT invite_tokens_workflow_id_fkey FOREIGN KEY (workflow_id) REFERENCES public.workflows(id) ON DELETE RESTRICT;


--
-- Name: mail_accounts mail_accounts_owner_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.mail_accounts
    ADD CONSTRAINT mail_accounts_owner_id_fkey FOREIGN KEY (owner_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: mail_attachments mail_attachments_file_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.mail_attachments
    ADD CONSTRAINT mail_attachments_file_id_fkey FOREIGN KEY (file_id) REFERENCES public.files(id) ON DELETE SET NULL;


--
-- Name: mail_attachments mail_attachments_message_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.mail_attachments
    ADD CONSTRAINT mail_attachments_message_id_fkey FOREIGN KEY (message_id) REFERENCES public.mail_messages(id) ON DELETE CASCADE;


--
-- Name: mail_import_jobs mail_import_jobs_account_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.mail_import_jobs
    ADD CONSTRAINT mail_import_jobs_account_id_fkey FOREIGN KEY (account_id) REFERENCES public.mail_accounts(id) ON DELETE CASCADE;


--
-- Name: mail_import_jobs mail_import_jobs_owner_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.mail_import_jobs
    ADD CONSTRAINT mail_import_jobs_owner_id_fkey FOREIGN KEY (owner_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: mail_links mail_links_created_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.mail_links
    ADD CONSTRAINT mail_links_created_by_fkey FOREIGN KEY (created_by) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: mail_links mail_links_message_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.mail_links
    ADD CONSTRAINT mail_links_message_id_fkey FOREIGN KEY (message_id) REFERENCES public.mail_messages(id) ON DELETE CASCADE;


--
-- Name: mail_message_parts mail_message_parts_message_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.mail_message_parts
    ADD CONSTRAINT mail_message_parts_message_id_fkey FOREIGN KEY (message_id) REFERENCES public.mail_messages(id) ON DELETE CASCADE;


--
-- Name: mail_messages mail_messages_archive_job_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.mail_messages
    ADD CONSTRAINT mail_messages_archive_job_id_fkey FOREIGN KEY (archive_job_id) REFERENCES public.mail_import_jobs(id) ON DELETE SET NULL;


--
-- Name: mail_messages mail_messages_folder_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.mail_messages
    ADD CONSTRAINT mail_messages_folder_id_fkey FOREIGN KEY (folder_id) REFERENCES public.folders(id) ON DELETE SET NULL;


--
-- Name: mail_messages mail_messages_imported_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.mail_messages
    ADD CONSTRAINT mail_messages_imported_by_fkey FOREIGN KEY (imported_by) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: mail_messages mail_messages_owner_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.mail_messages
    ADD CONSTRAINT mail_messages_owner_id_fkey FOREIGN KEY (owner_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: mail_send_idempotency mail_send_idempotency_account_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.mail_send_idempotency
    ADD CONSTRAINT mail_send_idempotency_account_id_fkey FOREIGN KEY (account_id) REFERENCES public.mail_accounts(id) ON DELETE CASCADE;


--
-- Name: mail_send_idempotency mail_send_idempotency_message_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.mail_send_idempotency
    ADD CONSTRAINT mail_send_idempotency_message_id_fkey FOREIGN KEY (message_id) REFERENCES public.mail_messages(id) ON DELETE SET NULL;


--
-- Name: mail_smtp_settings mail_smtp_settings_mail_account_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.mail_smtp_settings
    ADD CONSTRAINT mail_smtp_settings_mail_account_id_fkey FOREIGN KEY (mail_account_id) REFERENCES public.mail_accounts(id) ON DELETE CASCADE;


--
-- Name: mail_smtp_settings mail_smtp_settings_owner_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.mail_smtp_settings
    ADD CONSTRAINT mail_smtp_settings_owner_id_fkey FOREIGN KEY (owner_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: meeting_decision_creations meeting_decision_creations_meeting_folder_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_decision_creations
    ADD CONSTRAINT meeting_decision_creations_meeting_folder_id_fkey FOREIGN KEY (meeting_folder_id) REFERENCES public.folders(id) ON DELETE CASCADE;


--
-- Name: meeting_resource_links meeting_resource_links_meeting_folder_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_resource_links
    ADD CONSTRAINT meeting_resource_links_meeting_folder_id_fkey FOREIGN KEY (meeting_folder_id) REFERENCES public.folders(id) ON DELETE CASCADE;


--
-- Name: memory_files_reconcile_jobs memory_files_reconcile_jobs_tenant_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.memory_files_reconcile_jobs
    ADD CONSTRAINT memory_files_reconcile_jobs_tenant_id_fkey FOREIGN KEY (tenant_id) REFERENCES public.tenants(id) ON DELETE CASCADE;


--
-- Name: notifications notifications_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.notifications
    ADD CONSTRAINT notifications_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: oidc_config oidc_config_updated_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oidc_config
    ADD CONSTRAINT oidc_config_updated_by_fkey FOREIGN KEY (updated_by) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: organization_invite_resend_attempts organization_invite_resend_attem_tenant_id_invite_token_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.organization_invite_resend_attempts
    ADD CONSTRAINT organization_invite_resend_attem_tenant_id_invite_token_id_fkey FOREIGN KEY (tenant_id, invite_token_id) REFERENCES public.invite_tokens(tenant_id, id) ON DELETE CASCADE;


--
-- Name: organization_invite_resend_attempts organization_invite_resend_attempts_actor_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.organization_invite_resend_attempts
    ADD CONSTRAINT organization_invite_resend_attempts_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: replication_attempts replication_attempts_job_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.replication_attempts
    ADD CONSTRAINT replication_attempts_job_id_fkey FOREIGN KEY (job_id) REFERENCES public.replication_jobs(id) ON DELETE CASCADE;


--
-- Name: replication_attempts replication_attempts_target_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.replication_attempts
    ADD CONSTRAINT replication_attempts_target_id_fkey FOREIGN KEY (target_id) REFERENCES public.replication_targets(id) ON DELETE CASCADE;


--
-- Name: replication_jobs replication_jobs_file_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.replication_jobs
    ADD CONSTRAINT replication_jobs_file_id_fkey FOREIGN KEY (file_id) REFERENCES public.files(id) ON DELETE CASCADE;


--
-- Name: replication_jobs replication_jobs_file_version_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.replication_jobs
    ADD CONSTRAINT replication_jobs_file_version_id_fkey FOREIGN KEY (file_version_id) REFERENCES public.file_versions(id) ON DELETE CASCADE;


--
-- Name: share_access_log share_access_log_share_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.share_access_log
    ADD CONSTRAINT share_access_log_share_id_fkey FOREIGN KEY (share_id) REFERENCES public.shares(id) ON DELETE CASCADE;


--
-- Name: share_access_notifications share_access_notifications_share_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.share_access_notifications
    ADD CONSTRAINT share_access_notifications_share_id_fkey FOREIGN KEY (share_id) REFERENCES public.shares(id) ON DELETE CASCADE;


--
-- Name: share_access_notifications share_access_notifications_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.share_access_notifications
    ADD CONSTRAINT share_access_notifications_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: shares shares_created_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.shares
    ADD CONSTRAINT shares_created_by_fkey FOREIGN KEY (created_by) REFERENCES public.users(id);


--
-- Name: shares shares_file_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.shares
    ADD CONSTRAINT shares_file_id_fkey FOREIGN KEY (file_id) REFERENCES public.files(id) ON DELETE CASCADE;


--
-- Name: shares shares_folder_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.shares
    ADD CONSTRAINT shares_folder_id_fkey FOREIGN KEY (folder_id) REFERENCES public.folders(id) ON DELETE CASCADE;


--
-- Name: shares shares_recipient_group_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.shares
    ADD CONSTRAINT shares_recipient_group_id_fkey FOREIGN KEY (recipient_group_id) REFERENCES public.user_groups(id) ON DELETE CASCADE;


--
-- Name: shares shares_recipient_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.shares
    ADD CONSTRAINT shares_recipient_user_id_fkey FOREIGN KEY (recipient_user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: smtp_config smtp_config_updated_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.smtp_config
    ADD CONSTRAINT smtp_config_updated_by_fkey FOREIGN KEY (updated_by) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: templates templates_created_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.templates
    ADD CONSTRAINT templates_created_by_fkey FOREIGN KEY (created_by) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: upstream_buzz_mapping_operations upstream_buzz_mapping_operations_tenant_id_mapping_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.upstream_buzz_mapping_operations
    ADD CONSTRAINT upstream_buzz_mapping_operations_tenant_id_mapping_id_fkey FOREIGN KEY (tenant_id, mapping_id) REFERENCES public.upstream_buzz_tenant_relays(tenant_id, mapping_id) ON DELETE RESTRICT;


--
-- Name: upstream_buzz_relay_mapping_audit upstream_buzz_relay_mapping_audit_operation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.upstream_buzz_relay_mapping_audit
    ADD CONSTRAINT upstream_buzz_relay_mapping_audit_operation_id_fkey FOREIGN KEY (operation_id) REFERENCES public.upstream_buzz_mapping_operations(operation_id) ON DELETE RESTRICT;


--
-- Name: upstream_buzz_relay_mapping_audit upstream_buzz_relay_mapping_audit_tenant_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.upstream_buzz_relay_mapping_audit
    ADD CONSTRAINT upstream_buzz_relay_mapping_audit_tenant_id_fkey FOREIGN KEY (tenant_id) REFERENCES public.tenants(id) ON DELETE RESTRICT;


--
-- Name: upstream_buzz_relay_mapping_audit upstream_buzz_relay_mapping_audit_tenant_id_mapping_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.upstream_buzz_relay_mapping_audit
    ADD CONSTRAINT upstream_buzz_relay_mapping_audit_tenant_id_mapping_id_fkey FOREIGN KEY (tenant_id, mapping_id) REFERENCES public.upstream_buzz_tenant_relays(tenant_id, mapping_id) ON DELETE RESTRICT;


--
-- Name: upstream_buzz_tenant_relays upstream_buzz_tenant_relays_audit_fk; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.upstream_buzz_tenant_relays
    ADD CONSTRAINT upstream_buzz_tenant_relays_audit_fk FOREIGN KEY (audit_entry_id) REFERENCES public.upstream_buzz_relay_mapping_audit(audit_entry_id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: upstream_buzz_tenant_relays upstream_buzz_tenant_relays_tenant_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.upstream_buzz_tenant_relays
    ADD CONSTRAINT upstream_buzz_tenant_relays_tenant_id_fkey FOREIGN KEY (tenant_id) REFERENCES public.tenants(id) ON DELETE RESTRICT;


--
-- Name: upstream_buzz_workspace_relay_bindings upstream_buzz_workspace_bindings_audit_fk; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.upstream_buzz_workspace_relay_bindings
    ADD CONSTRAINT upstream_buzz_workspace_bindings_audit_fk FOREIGN KEY (audit_entry_id) REFERENCES public.upstream_buzz_relay_mapping_audit(audit_entry_id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: upstream_buzz_workspace_relay_bindings upstream_buzz_workspace_relay_binding_tenant_id_mapping_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.upstream_buzz_workspace_relay_bindings
    ADD CONSTRAINT upstream_buzz_workspace_relay_binding_tenant_id_mapping_id_fkey FOREIGN KEY (tenant_id, mapping_id) REFERENCES public.upstream_buzz_tenant_relays(tenant_id, mapping_id) ON DELETE RESTRICT;


--
-- Name: user_groups user_groups_created_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_groups
    ADD CONSTRAINT user_groups_created_by_fkey FOREIGN KEY (created_by) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: user_security_events user_security_events_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_security_events
    ADD CONSTRAINT user_security_events_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: user_sessions user_sessions_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_sessions
    ADD CONSTRAINT user_sessions_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: vault_devices vault_devices_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.vault_devices
    ADD CONSTRAINT vault_devices_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: vault_devices vault_devices_vault_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.vault_devices
    ADD CONSTRAINT vault_devices_vault_id_fkey FOREIGN KEY (vault_id) REFERENCES public.vaults(id) ON DELETE SET NULL;


--
-- Name: vault_files vault_files_vault_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.vault_files
    ADD CONSTRAINT vault_files_vault_id_fkey FOREIGN KEY (vault_id) REFERENCES public.vaults(id) ON DELETE CASCADE;


--
-- Name: vaults vaults_owner_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.vaults
    ADD CONSTRAINT vaults_owner_user_id_fkey FOREIGN KEY (owner_user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: webhook_configs webhook_configs_created_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.webhook_configs
    ADD CONSTRAINT webhook_configs_created_by_fkey FOREIGN KEY (created_by) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: webhook_logs webhook_logs_webhook_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.webhook_logs
    ADD CONSTRAINT webhook_logs_webhook_id_fkey FOREIGN KEY (webhook_id) REFERENCES public.webhook_configs(id) ON DELETE CASCADE;


--
-- Name: workflows workflows_updated_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.workflows
    ADD CONSTRAINT workflows_updated_by_fkey FOREIGN KEY (updated_by) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: file_versions; Type: ROW SECURITY; Schema: public; Owner: -
--

ALTER TABLE public.file_versions ENABLE ROW LEVEL SECURITY;

--
-- Name: file_versions file_versions_owner_isolation; Type: POLICY; Schema: public; Owner: -
--

CREATE POLICY file_versions_owner_isolation ON public.file_versions USING (((current_setting('app.current_user_id'::text, true) IS NULL) OR (current_setting('app.current_user_id'::text, true) = '00000000-0000-0000-0000-000000000000'::text) OR (file_id IN ( SELECT files.id
   FROM public.files
  WHERE (files.owner_id = (current_setting('app.current_user_id'::text, true))::uuid)))));


--
-- Name: files; Type: ROW SECURITY; Schema: public; Owner: -
--

ALTER TABLE public.files ENABLE ROW LEVEL SECURITY;

--
-- Name: files files_owner_isolation; Type: POLICY; Schema: public; Owner: -
--

CREATE POLICY files_owner_isolation ON public.files USING (((current_setting('app.current_user_id'::text, true) IS NULL) OR (current_setting('app.current_user_id'::text, true) = '00000000-0000-0000-0000-000000000000'::text) OR (owner_id = (current_setting('app.current_user_id'::text, true))::uuid)));


--
-- Name: folders; Type: ROW SECURITY; Schema: public; Owner: -
--

ALTER TABLE public.folders ENABLE ROW LEVEL SECURITY;

--
-- Name: folders folders_owner_isolation; Type: POLICY; Schema: public; Owner: -
--

CREATE POLICY folders_owner_isolation ON public.folders USING (((current_setting('app.current_user_id'::text, true) IS NULL) OR (current_setting('app.current_user_id'::text, true) = '00000000-0000-0000-0000-000000000000'::text) OR (owner_id = (current_setting('app.current_user_id'::text, true))::uuid)));


--
-- Name: managed_chat_identities; Type: ROW SECURITY; Schema: public; Owner: -
--

ALTER TABLE public.managed_chat_identities ENABLE ROW LEVEL SECURITY;

--
-- Name: managed_chat_identities managed_chat_identities_tenant_isolation; Type: POLICY; Schema: public; Owner: -
--

CREATE POLICY managed_chat_identities_tenant_isolation ON public.managed_chat_identities USING ((tenant_id = (NULLIF(current_setting('app.current_tenant_id'::text, true), ''::text))::uuid)) WITH CHECK ((tenant_id = (NULLIF(current_setting('app.current_tenant_id'::text, true), ''::text))::uuid));


--
-- Name: managed_chat_identity_operations; Type: ROW SECURITY; Schema: public; Owner: -
--

ALTER TABLE public.managed_chat_identity_operations ENABLE ROW LEVEL SECURITY;

--
-- Name: managed_chat_identity_operations managed_chat_identity_operations_tenant_isolation; Type: POLICY; Schema: public; Owner: -
--

CREATE POLICY managed_chat_identity_operations_tenant_isolation ON public.managed_chat_identity_operations USING ((tenant_id = (NULLIF(current_setting('app.current_tenant_id'::text, true), ''::text))::uuid)) WITH CHECK ((tenant_id = (NULLIF(current_setting('app.current_tenant_id'::text, true), ''::text))::uuid));


--
-- Name: upstream_buzz_mapping_operations; Type: ROW SECURITY; Schema: public; Owner: -
--

ALTER TABLE public.upstream_buzz_mapping_operations ENABLE ROW LEVEL SECURITY;

--
-- Name: upstream_buzz_mapping_operations upstream_buzz_mapping_operations_tenant_isolation; Type: POLICY; Schema: public; Owner: -
--

CREATE POLICY upstream_buzz_mapping_operations_tenant_isolation ON public.upstream_buzz_mapping_operations USING ((tenant_id = (NULLIF(current_setting('app.current_tenant_id'::text, true), ''::text))::uuid)) WITH CHECK ((tenant_id = (NULLIF(current_setting('app.current_tenant_id'::text, true), ''::text))::uuid));


--
-- Name: upstream_buzz_relay_mapping_audit; Type: ROW SECURITY; Schema: public; Owner: -
--

ALTER TABLE public.upstream_buzz_relay_mapping_audit ENABLE ROW LEVEL SECURITY;

--
-- Name: upstream_buzz_relay_mapping_audit upstream_buzz_relay_mapping_audit_tenant_isolation; Type: POLICY; Schema: public; Owner: -
--

CREATE POLICY upstream_buzz_relay_mapping_audit_tenant_isolation ON public.upstream_buzz_relay_mapping_audit USING ((tenant_id = (NULLIF(current_setting('app.current_tenant_id'::text, true), ''::text))::uuid)) WITH CHECK ((tenant_id = (NULLIF(current_setting('app.current_tenant_id'::text, true), ''::text))::uuid));


--
-- Name: upstream_buzz_tenant_relays; Type: ROW SECURITY; Schema: public; Owner: -
--

ALTER TABLE public.upstream_buzz_tenant_relays ENABLE ROW LEVEL SECURITY;

--
-- Name: upstream_buzz_tenant_relays upstream_buzz_tenant_relays_tenant_isolation; Type: POLICY; Schema: public; Owner: -
--

CREATE POLICY upstream_buzz_tenant_relays_tenant_isolation ON public.upstream_buzz_tenant_relays USING ((tenant_id = (NULLIF(current_setting('app.current_tenant_id'::text, true), ''::text))::uuid)) WITH CHECK ((tenant_id = (NULLIF(current_setting('app.current_tenant_id'::text, true), ''::text))::uuid));


--
-- Name: upstream_buzz_workspace_relay_bindings; Type: ROW SECURITY; Schema: public; Owner: -
--

ALTER TABLE public.upstream_buzz_workspace_relay_bindings ENABLE ROW LEVEL SECURITY;

--
-- Name: upstream_buzz_workspace_relay_bindings upstream_buzz_workspace_relay_bindings_tenant_isolation; Type: POLICY; Schema: public; Owner: -
--

CREATE POLICY upstream_buzz_workspace_relay_bindings_tenant_isolation ON public.upstream_buzz_workspace_relay_bindings USING ((tenant_id = (NULLIF(current_setting('app.current_tenant_id'::text, true), ''::text))::uuid)) WITH CHECK ((tenant_id = (NULLIF(current_setting('app.current_tenant_id'::text, true), ''::text))::uuid));


--
-- Name: vault_devices; Type: ROW SECURITY; Schema: public; Owner: -
--

ALTER TABLE public.vault_devices ENABLE ROW LEVEL SECURITY;

--
-- Name: vault_devices vault_devices_owner_isolation; Type: POLICY; Schema: public; Owner: -
--

CREATE POLICY vault_devices_owner_isolation ON public.vault_devices USING (((current_setting('app.current_user_id'::text, true) IS NULL) OR (current_setting('app.current_user_id'::text, true) = '00000000-0000-0000-0000-000000000000'::text) OR (user_id = (current_setting('app.current_user_id'::text, true))::uuid)));


--
-- Name: vault_files; Type: ROW SECURITY; Schema: public; Owner: -
--

ALTER TABLE public.vault_files ENABLE ROW LEVEL SECURITY;

--
-- Name: vault_files vault_files_owner_isolation; Type: POLICY; Schema: public; Owner: -
--

CREATE POLICY vault_files_owner_isolation ON public.vault_files USING (((current_setting('app.current_user_id'::text, true) IS NULL) OR (current_setting('app.current_user_id'::text, true) = '00000000-0000-0000-0000-000000000000'::text) OR (vault_id IN ( SELECT vaults.id
   FROM public.vaults
  WHERE (vaults.owner_user_id = (current_setting('app.current_user_id'::text, true))::uuid)))));


--
-- Name: vaults; Type: ROW SECURITY; Schema: public; Owner: -
--

ALTER TABLE public.vaults ENABLE ROW LEVEL SECURITY;

--
-- Name: vaults vaults_owner_isolation; Type: POLICY; Schema: public; Owner: -
--

CREATE POLICY vaults_owner_isolation ON public.vaults USING (((current_setting('app.current_user_id'::text, true) IS NULL) OR (current_setting('app.current_user_id'::text, true) = '00000000-0000-0000-0000-000000000000'::text) OR (owner_user_id = (current_setting('app.current_user_id'::text, true))::uuid)));


--
-- PostgreSQL database dump complete
--


