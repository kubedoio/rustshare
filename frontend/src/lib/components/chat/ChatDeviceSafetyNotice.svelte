<section aria-labelledby="chat-device-safety-title">
	<h2 id="chat-device-safety-title" class="mb-3 text-lg font-semibold">
		Chat devices and lost devices
	</h2>
	<div class="rounded-lg border border-warning/30 bg-warning/10 p-4 text-sm">
		<p class="font-medium">Elembra credentials and Buzz keys are different</p>
		<p class="mt-2">
			Browser sessions and device tokens let a browser or client authenticate to Elembra. A device
			token does not prove that a particular device holds your Buzz signing key. Revoking a
			credential stops its future Elembra authentication; it cannot erase a copied key or invalidate
			signatures made with that key.
		</p>
		<p class="mt-2">
			If a missing device may have had Chat unlocked, or an encrypted backup and its passphrase may
			both be exposed, treat the shared Buzz key as compromised. Locking Chat here only clears this
			browser's in-memory key. Contact your workspace administrator to coordinate revocation of the
			old Buzz admission. Identity replacement is currently unavailable; this page cannot replace
			it.
		</p>
		<div class="mt-3 flex flex-wrap gap-4">
			<a class="font-medium text-brand-500 hover:underline" href="/settings?tab=security">
				Review browser sessions
			</a>
			<a class="font-medium text-brand-500 hover:underline" href="/settings?tab=devices">
				Review device tokens
			</a>
		</div>
	</div>
</section>
