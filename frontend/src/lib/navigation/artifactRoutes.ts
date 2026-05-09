import { goto } from '$app/navigation';

export function navigateToNote(noteId: string) {
	goto(`/modules/notes/${noteId}`);
}

export function navigateToMeeting(meetingId: string) {
	goto(`/modules/meetings/${meetingId}`);
}

export function navigateToStandup(standupId: string) {
	goto(`/modules/standups/${standupId}`);
}

export function navigateToDecision(decisionId: string) {
	goto(`/modules/decisions/${decisionId}`);
}

export function navigateToKanbanBoard(boardId: string) {
	goto(`/modules/kanban/${boardId}`);
}

export function navigateToBrainstormingBoard(boardId: string) {
	goto(`/modules/brainstorming/${boardId}`);
}

export function navigateToSharePackage(shareId: string) {
	goto(`/modules/shares/${shareId}`);
}

/**
 * Generic module artifact navigator.
 */
export function navigateToModuleArtifact(moduleKey: string, artifactId: string) {
	const routeMap: Record<string, (id: string) => void> = {
		notes: navigateToNote,
		meetings: navigateToMeeting,
		standups: navigateToStandup,
		decisions: navigateToDecision,
		kanban: navigateToKanbanBoard,
		brainstorming: navigateToBrainstormingBoard,
		shares: navigateToSharePackage
	};
	const navigator = routeMap[moduleKey];
	if (navigator) {
		navigator(artifactId);
	} else {
		goto(`/modules/${moduleKey}/${artifactId}`);
	}
}
