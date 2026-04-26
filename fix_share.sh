#!/bin/bash
sed -i 's/enabled: open && activeTab === '"'"'share'"'"'/enabled: open \&\& activeTab === ('"'"'share'"'"' as any)/g' frontend/src/lib/components/modals/ShareModal.svelte
