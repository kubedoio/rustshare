document.addEventListener('DOMContentLoaded', () => {
  // Select DOM Elements
  const sections = document.querySelectorAll('.doc-section');
  const navItems = document.querySelectorAll('.nav-item');
  const sidebar = document.querySelector('.sidebar');
  const menuToggle = document.querySelector('.menu-toggle');
  const mainWrapper = document.querySelector('.main-wrapper');
  
  // Theme Toggle Buttons
  const themeBtns = document.querySelectorAll('.theme-btn');
  
  // Search DOM Elements
  const searchTrigger = document.querySelector('.search-trigger');
  const searchModal = document.querySelector('.search-modal');
  const searchClose = document.querySelector('.search-close');
  const searchInput = document.querySelector('.search-input');
  const searchResults = document.querySelector('.search-results');

  /* ==========================================
     1. Hash-based Router
     ========================================== */
  function handleRouting() {
    let hash = window.location.hash || '#home';
    
    // Validate hash, fallback to home if not matching any section
    let activeSection = document.querySelector(hash);
    if (!activeSection || !activeSection.classList.contains('doc-section')) {
      hash = '#home';
      activeSection = document.getElementById('home');
    }

    // Toggle active classes on sections
    sections.forEach(sec => {
      sec.classList.remove('active-section');
    });
    activeSection.classList.add('active-section');

    // Toggle active classes on navigation items
    navItems.forEach(item => {
      const link = item.querySelector('a');
      if (link && link.getAttribute('href') === hash) {
        item.classList.add('active');
      } else {
        item.classList.remove('active');
      }
    });

    // Reset scroll of content wrapper to top
    mainWrapper.scrollTop = 0;

    // Close mobile drawer on routing
    if (sidebar.classList.contains('active')) {
      sidebar.classList.remove('active');
    }
  }

  // Bind Router Events
  window.addEventListener('hashchange', handleRouting);
  // Run router immediately on initial load
  handleRouting();

  /* ==========================================
     2. Mobile Navigation Drawer Toggle
     ========================================== */
  if (menuToggle) {
    menuToggle.addEventListener('click', (e) => {
      e.stopPropagation();
      sidebar.classList.toggle('active');
    });
  }

  // Close sidebar drawer if clicking outside on mobile
  document.addEventListener('click', (e) => {
    if (window.innerWidth <= 768 && sidebar.classList.contains('active')) {
      if (!sidebar.contains(e.target) && !menuToggle.contains(e.target)) {
        sidebar.classList.remove('active');
      }
    }
  });

  /* ==========================================
     3. Dual-Theme Management (System, Light, Dark)
     ========================================== */
  function applyTheme(theme) {
    const root = document.documentElement;
    const systemDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
    
    // Handle html class
    if (theme === 'dark' || (theme === 'system' && systemDark)) {
      root.classList.add('dark');
    } else {
      root.classList.remove('dark');
    }

    // Highlight active button in theme selector
    themeBtns.forEach(btn => {
      if (btn.dataset.theme === theme) {
        btn.classList.add('active');
      } else {
        btn.classList.remove('active');
      }
    });

    // Persistent storage
    localStorage.setItem('rustshare-docs-theme', theme);
  }

  // Initialize theme from storage or default to system
  const savedTheme = localStorage.getItem('rustshare-docs-theme') || 'system';
  applyTheme(savedTheme);

  // Bind theme selector clicks
  themeBtns.forEach(btn => {
    btn.addEventListener('click', () => {
      applyTheme(btn.dataset.theme);
    });
  });

  // Listen to system theme changes in background
  window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
    if (localStorage.getItem('rustshare-docs-theme') === 'system') {
      applyTheme('system');
    }
  });

  /* ==========================================
     4. Client-Side Instant Search Indexer
     ========================================== */
  let searchIndex = [];

  function buildSearchIndex() {
    searchIndex = [];
    
    // Traverse through all sections and collect search items
    sections.forEach(section => {
      const sectionId = section.id;
      // Get human-readable page name
      const sectionTitle = section.dataset.title || section.querySelector('h1')?.innerText || sectionId;

      // Index section headings (H2, H3)
      const headings = section.querySelectorAll('h2, h3');
      headings.forEach(h => {
        // Create an ID for the heading if it doesn't exist to allow direct anchor linking
        if (!h.id) {
          h.id = h.innerText.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/(^-|-$)/g, '');
        }

        searchIndex.push({
          sectionId: sectionId,
          sectionTitle: sectionTitle,
          anchorId: h.id,
          title: h.innerText,
          text: h.innerText,
          type: 'heading'
        });
      });

      // Index paragraphs and list items
      const textBlocks = section.querySelectorAll('p, li, td');
      textBlocks.forEach(block => {
        // Skip text block if it is inside tables headers or navigation
        if (block.closest('thead') || block.closest('.quick-nav-grid') || block.innerText.trim().length < 10) {
          return;
        }

        // Find nearest heading above this block to give it a sensible label
        let nearestHeading = '';
        let prev = block.previousElementSibling;
        while (prev) {
          if (prev.tagName === 'H1' || prev.tagName === 'H2' || prev.tagName === 'H3') {
            nearestHeading = prev.innerText;
            break;
          }
          prev = prev.previousElementSibling;
        }

        searchIndex.push({
          sectionId: sectionId,
          sectionTitle: sectionTitle,
          anchorId: block.id || '',
          title: nearestHeading || sectionTitle,
          text: block.innerText,
          type: 'body'
        });
      });
    });
  }

  // Initialize Search Index
  buildSearchIndex();

  // Search Logic
  function performSearch(query) {
    query = query.trim().toLowerCase();
    searchResults.innerHTML = '';

    if (!query) {
      searchResults.innerHTML = '<li class="search-empty">Type something to search documentation...</li>';
      return;
    }

    // Filter index entries matching the query string
    const matches = [];
    searchIndex.forEach(item => {
      const textMatch = item.text.toLowerCase().indexOf(query);
      const titleMatch = item.title.toLowerCase().indexOf(query);

      if (textMatch !== -1 || titleMatch !== -1) {
        // Score the match for sorting
        let score = 0;
        if (titleMatch !== -1) score += 10; // Higher weight for title matches
        if (item.type === 'heading') score += 5; // Higher weight for headings
        
        // Find match position
        const matchIdx = textMatch !== -1 ? textMatch : titleMatch;

        matches.push({
          ...item,
          score: score,
          matchIndex: matchIdx
        });
      }
    });

    // Sort results by score (descending)
    matches.sort((a, b) => b.score - a.score);

    // Limit to 8 high-relevancy results
    const limitedMatches = matches.slice(0, 8);

    if (limitedMatches.length === 0) {
      searchResults.innerHTML = '<li class="search-empty">No results found for "' + escapeHtml(query) + '"</li>';
      return;
    }

    // Generate HTML for results
    limitedMatches.forEach(match => {
      const li = document.createElement('li');
      li.className = 'search-result-item';

      const targetHref = '#' + match.sectionId + (match.anchorId ? '#' + match.anchorId : '');
      
      // Construct technical text snippet highlight
      let snippet = match.text;
      if (snippet.length > 120) {
        const start = Math.max(0, match.matchIndex - 40);
        const end = Math.min(snippet.length, match.matchIndex + 80);
        snippet = (start > 0 ? '...' : '') + snippet.substring(start, end) + (end < snippet.length ? '...' : '');
      }

      // Highlight matched query in title and snippet
      const highlightedTitle = highlightTerm(match.title, query);
      const highlightedSnippet = highlightTerm(snippet, query);

      li.innerHTML = `
        <a href="${targetHref}">
          <div class="search-result-section">${escapeHtml(match.sectionTitle)}</div>
          <div class="search-result-title">${highlightedTitle}</div>
          <div class="search-result-snippet">${highlightedSnippet}</div>
        </a>
      `;

      // Bind click handler to close modal and navigate
      li.querySelector('a').addEventListener('click', (e) => {
        e.preventDefault();
        closeSearch();
        
        // Push hash state to router
        window.location.hash = match.sectionId;
        
        // If it is a heading anchor link, scroll it into view after a tiny layout tick
        if (match.anchorId) {
          setTimeout(() => {
            const el = document.getElementById(match.anchorId);
            if (el) {
              el.scrollIntoView({ behavior: 'smooth', block: 'start' });
            }
          }, 50);
        }
      });

      searchResults.appendChild(li);
    });
  }

  // Helper Escapers
  function escapeHtml(text) {
    return text
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;")
      .replace(/'/g, "&#039;");
  }

  function highlightTerm(text, query) {
    const escaped = escapeHtml(text);
    const idx = escaped.toLowerCase().indexOf(query);
    if (idx === -1) return escaped;

    const originalTerm = escaped.substring(idx, idx + query.length);
    return escaped.substring(0, idx) + 
      `<mark style="background-color: var(--color-brand-subtle); color: var(--color-brand); border-radius: 2px; padding: 0 2px; font-weight: 600;">${originalTerm}</mark>` + 
      escaped.substring(idx + query.length);
  }

  // Open / Close Modal Handlers
  function openSearch() {
    searchModal.classList.add('active');
    document.body.style.overflow = 'hidden'; // Lock background scroll
    setTimeout(() => searchInput.focus(), 50);
    performSearch(searchInput.value);
  }

  function closeSearch() {
    searchModal.classList.remove('active');
    document.body.style.overflow = ''; // Restore background scroll
  }

  // Search Event Listeners
  if (searchTrigger) searchTrigger.addEventListener('click', openSearch);
  if (searchClose) searchClose.addEventListener('click', closeSearch);
  
  // Close modal when clicking dark overlay backdrop
  searchModal.addEventListener('click', (e) => {
    if (e.target === searchModal) {
      closeSearch();
    }
  });

  // Perform search on input change
  searchInput.addEventListener('input', (e) => {
    performSearch(e.target.value);
  });

  // Hotkeys binding
  document.addEventListener('keydown', (e) => {
    // '/' key opens search modal (if not currently focused inside input fields)
    if (e.key === '/' && document.activeElement !== searchInput && document.activeElement.tagName !== 'INPUT' && document.activeElement.tagName !== 'TEXTAREA') {
      e.preventDefault();
      openSearch();
    }
    
    // 'Escape' key closes modal
    if (e.key === 'Escape' && searchModal.classList.contains('active')) {
      closeSearch();
    }
  });

  /* ==========================================
     5. Clipboard Code Block Copy Buttons
     ========================================== */
  function injectCopyButtons() {
    const preBlocks = document.querySelectorAll('pre');
    
    preBlocks.forEach(pre => {
      // Create and inject copy button container inside relative pre
      const btn = document.createElement('button');
      btn.className = 'copy-btn';
      btn.innerHTML = `
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" class="copy-svg"><rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/></svg>
        <span>Copy</span>
      `;
      
      pre.appendChild(btn);

      btn.addEventListener('click', async () => {
        const codeElement = pre.querySelector('code');
        if (!codeElement) return;

        // Extract raw code text (removing line number labels if visual representation adds any)
        const codeText = codeElement.innerText;

        try {
          // Write to clipboard
          await navigator.clipboard.writeText(codeText);
          
          // Switch state to Copied!
          btn.innerHTML = `
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" style="color: var(--tip-border);"><polyline points="20 6 9 17 4 12"/></svg>
            <span style="color: var(--tip-border);">Copied!</span>
          `;
          btn.style.borderColor = 'var(--tip-border)';

          // Revert back after 2 seconds
          setTimeout(() => {
            btn.innerHTML = `
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" class="copy-svg"><rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/></svg>
              <span>Copy</span>
            `;
            btn.style.borderColor = '';
          }, 2000);
        } catch (err) {
          console.error('Failed to copy to clipboard', err);
          btn.querySelector('span').innerText = 'Error';
        }
      });
    });
  }

  // Inject buttons on load
  injectCopyButtons();
});
