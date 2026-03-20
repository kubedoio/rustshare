<script lang="ts">
  import { createMutation, createQuery } from '@tanstack/svelte-query';
  import { goto } from '$app/navigation';
  import {
    deleteNotification,
    listNotifications,
    markNotificationRead
  } from '$lib/api/notifications';
  import type { Notification } from '$lib/api/types';
  import { queryClient } from '$lib/query-client';
  import { toastStore } from '$lib/stores/toast';
  import { resolveNotificationTarget } from '$lib/utils/shared';
  import { formatDate } from '$lib/utils/format';

  let unreadOnly = false;

  $: notificationsQuery = createQuery({
    queryKey: ['notifications', unreadOnly],
    queryFn: () => listNotifications({ unreadOnly, limit: 100 })
  });

  const markReadMutation = createMutation({
    mutationFn: markNotificationRead,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['notifications'] });
    },
    onError: (error) => {
      toastStore.show(
        error instanceof Error ? error.message : 'Failed to mark notification as read',
        'error'
      );
    }
  });

  const deleteNotificationMutation = createMutation({
    mutationFn: deleteNotification,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['notifications'] });
      toastStore.show('Notification removed', 'success');
    },
    onError: (error) => {
      toastStore.show(
        error instanceof Error ? error.message : 'Failed to remove notification',
        'error'
      );
    }
  });

  function notificationIcon(notificationType: string): string {
    switch (notificationType) {
      case 'share_received':
        return '📨';
      case 'permission_changed':
        return '🔐';
      case 'share_revoked':
        return '🚫';
      default:
        return '🔔';
    }
  }

  function notificationBadgeClass(notificationType: string): string {
    switch (notificationType) {
      case 'share_received':
        return 'badge-info';
      case 'permission_changed':
        return 'badge-warning';
      case 'share_revoked':
        return 'badge-error';
      default:
        return 'badge-ghost';
    }
  }

  function notificationLabel(notificationType: string): string {
    if (notificationType === 'share_received') return 'Share received';
    if (notificationType === 'permission_changed') return 'Permission changed';
    if (notificationType === 'share_revoked') return 'Share revoked';
    return 'Notification';
  }

  async function handleOpenNotification(notification: Notification) {
    if (!notification.read) {
      try {
        await $markReadMutation.mutateAsync(notification.id);
      } catch {
        // Navigation still provides value even if the mark-read call fails.
      }
    }

    goto(resolveNotificationTarget(notification));
  }
</script>

<svelte:head>
  <title>Notifications - RustShare</title>
</svelte:head>

<div class="space-y-6">
  <div class="flex items-start justify-between gap-4">
    <div>
      <h1 class="text-3xl font-bold">Notifications</h1>
      <p class="mt-1 text-base-content/70">
        Persistent share and permission updates for your account
      </p>
    </div>

    <label class="label cursor-pointer gap-3">
      <span class="label-text">Unread only</span>
      <input type="checkbox" class="toggle" bind:checked={unreadOnly} />
    </label>
  </div>

  {#if $notificationsQuery.isLoading}
    <div class="py-12 flex justify-center">
      <span class="loading loading-spinner loading-lg"></span>
    </div>
  {:else if $notificationsQuery.isError}
    <div class="alert alert-error">
      <span>Failed to load notifications: {$notificationsQuery.error?.message}</span>
    </div>
  {:else if $notificationsQuery.data && $notificationsQuery.data.notifications.length === 0}
    <div class="card bg-base-100 shadow-xl">
      <div class="card-body flex flex-col items-center justify-center py-16 text-center">
        <div class="text-6xl mb-4">🔔</div>
        <h2 class="text-2xl font-bold mb-2">No notifications yet</h2>
        <p class="max-w-md text-base-content/70 mb-6">
          When someone shares a file or folder with you, changes your permission, or revokes
          access, it will appear here.
        </p>
        <button class="btn btn-primary" on:click={() => goto('/shared-with-me')}>
          Go to Shared with Me
        </button>
      </div>
    </div>
  {:else if $notificationsQuery.data}
    <div class="card bg-base-100 shadow-xl">
      <div class="card-body p-0">
        <div class="px-6 pt-6 pb-2 flex items-center justify-between gap-4">
          <div>
            <h2 class="card-title">Inbox</h2>
            <p class="text-sm text-base-content/70">
              {$notificationsQuery.data.total} notification{$notificationsQuery.data.total === 1 ? '' : 's'}
            </p>
          </div>
          <button class="btn btn-outline btn-sm" on:click={() => goto('/shared-with-me')}>
            Shared with Me
          </button>
        </div>

        <div class="divide-y divide-base-200">
          {#each $notificationsQuery.data.notifications as notification}
            <div class={`px-6 py-5 ${notification.read ? '' : 'bg-base-200/40'}`}>
              <div class="flex items-start gap-4">
                <div class="text-2xl leading-none pt-1">{notificationIcon(notification.notification_type)}</div>

                <div class="flex-1 min-w-0 space-y-2">
                  <div class="flex items-center gap-2 flex-wrap">
                    <h3 class="font-semibold">{notification.title}</h3>
                    <span class={`badge badge-sm ${notificationBadgeClass(notification.notification_type)}`}>
                      {notificationLabel(notification.notification_type)}
                    </span>
                    {#if !notification.read}
                      <span class="badge badge-sm badge-primary">Unread</span>
                    {/if}
                  </div>

                  <p class="text-sm text-base-content/80">{notification.message}</p>

                  <div class="text-xs text-base-content/60">
                    {formatDate(notification.created_at)}
                  </div>
                </div>

                <div class="flex items-center gap-2">
                  {#if !notification.read}
                    <button
                      class="btn btn-ghost btn-sm"
                      on:click={() => $markReadMutation.mutate(notification.id)}
                      disabled={$markReadMutation.isPending}
                    >
                      Mark read
                    </button>
                  {/if}

                  <button
                    class="btn btn-outline btn-sm"
                    on:click={() => handleOpenNotification(notification)}
                  >
                    Open
                  </button>

                  <button
                    class="btn btn-ghost btn-sm text-error"
                    on:click={() => $deleteNotificationMutation.mutate(notification.id)}
                    disabled={$deleteNotificationMutation.isPending}
                  >
                    Remove
                  </button>
                </div>
              </div>
            </div>
          {/each}
        </div>
      </div>
    </div>
  {/if}
</div>
