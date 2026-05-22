/**
 * Cloudflare Worker for docs.rustshare.io
 * Serves static documentation files from the ASSETS binding.
 */
export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    
    // Serve index.html directly for the root directory path
    if (url.pathname === "/") {
      return env.ASSETS.fetch(new Request(new URL("/index.html", request.url)));
    }
    
    // Attempt to fetch the static asset from the Cloudflare Workers ASSETS binding
    try {
      const response = await env.ASSETS.fetch(request);
      
      // Clean fallback to index.html for SPA/clean path routes if resource is missing (404)
      if (response.status === 404) {
        return env.ASSETS.fetch(new Request(new URL("/index.html", request.url)));
      }
      
      return response;
    } catch (err) {
      // Fallback in case of internal errors fetching assets
      console.error("Asset fetch error:", err);
      return env.ASSETS.fetch(new Request(new URL("/index.html", request.url)));
    }
  }
};
