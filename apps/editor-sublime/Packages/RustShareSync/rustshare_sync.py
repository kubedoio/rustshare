import sublime
import sublime_plugin
import json
import http.client
import os

# Configuration (In a real app, this would be read from settings)
SYNC_SERVER = "localhost:4242"
RPC_TOKEN = "SECRET_TOKEN"

class RustShareSyncListener(sublime_plugin.EventListener):
    def on_post_save(self, view):
        file_path = view.file_name()
        if not file_path:
            return
        
        print(f"RustShareSync: Save detected for {file_path}. Triggering Sync...")
        self.trigger_sync(file_path)

    def trigger_sync(self, file_path):
        try:
            conn = http.client.HTTPConnection(SYNC_SERVER)
            headers = {
                "Content-Type": "application/json",
                "X-RustShare-Token": RPC_TOKEN
            }
            payload = {
                "jsonrpc": "2.0",
                "method": "sync.request",
                "params": {"path": file_path},
                "id": 1
            }
            conn.request("POST", "/rpc", body=json.dumps(payload), headers=headers)
            response = conn.getresponse()
            data = json.loads(response.read().decode())
            print(f"RustShareSync Response: {data}")
            conn.close()
        except Exception as e:
            print(f"RustShareSync Error: {str(e)}")

class RustShareSyncStatusCommand(sublime_plugin.TextCommand):
    def run(self, edit):
        file_path = self.view.file_name()
        if not file_path:
            sublime.message_dialog("RustShare: Current file not saved locally.")
            return

        print(f"RustShareSync: Querying status for {file_path}...")
        # Similar logic to trigger_sync but calling sync.status
        sublime.status_message(f"RustShare Sync: Checking status for {os.path.basename(file_path)}...")
