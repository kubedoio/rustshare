import { goto } from '$app/navigation';

export function navigateToNote(noteId: string, returnTo?: string) {
	const url = returnTo
		? `/apps/notes/${noteId}?returnTo=${encodeURIComponent(returnTo)}`
		: `/apps/notes/${noteId}`;
	goto(url);
}

export function navigateToMeeting(meetingId: string) {
	goto(`/apps/meetings/${meetingId}`);
}

export function navigateToStandup(standupId: string) {
	goto(`/apps/standups/${standupId}`);
}

export function navigateToDecision(decisionId: string) {
	goto(`/apps/decisions/${decisionId}`);
}

export function navigateToKanbanBoard(boardId: string) {
	goto(`/apps/kanban/${boardId}`);
}

export function navigateToBrainstormingBoard(boardId: string) {
	goto(`/apps/brainstorming/${boardId}`);
}

export function navigateToSharePackage(shareId: string) {
	goto(`/apps/shares/${shareId}`);
}

/**
 * Generic module artifact navigator.
 */
export function navigateToApplicationArtifact(applicationId: string, artifactId: string) {
	const routeMap: Record<string, (id: string) => void> = {
		notes: navigateToNote,
		meetings: navigateToMeeting,
		standups: navigateToStandup,
		decisions: navigateToDecision,
		kanban: navigateToKanbanBoard,
		brainstorming: navigateToBrainstormingBoard,
		shares: navigateToSharePackage
	};
	const navigator = routeMap[applicationId];
	if (navigator) {
		navigator(artifactId);
	} else {
		goto(`/apps/${applicationId}/${artifactId}`);
	}
}
