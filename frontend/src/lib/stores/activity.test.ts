import { describe, it, expect, beforeEach, vi } from 'vitest';
import { get } from 'svelte/store';
import {
  activityStore,
  getActivityDisplay,
  getRelativeTime,
  type Activity,
  type ActivityType
} from '$lib/stores/activity';

describe('Activity Store', () => {
  beforeEach(() => {
    activityStore.clearHistory();
    localStorage.clear();
  });

  describe('Initial State', () => {
    it('should start with empty history', () => {
      expect(get(activityStore)).toEqual([]);
    });

    it('should load from localStorage if available', () => {
      const mockActivities: Activity[] = [
        {
          id: '1',
          type: 'file_uploaded',
          fileName: 'test.txt',
          timestamp: new Date().toISOString()
        }
      ];

      localStorage.setItem('activity-history', JSON.stringify(mockActivities));

      // Re-import to trigger loadActivities
      const { activityStore: freshStore } = require('$lib/stores/activity');
      const activities = get(freshStore);

      expect(activities).toHaveLength(1);
      expect(activities[0].fileName).toBe('test.txt');
    });

    it('should handle corrupted localStorage gracefully', () => {
      localStorage.setItem('activity-history', 'invalid json');

      const { activityStore: freshStore } = require('$lib/stores/activity');
      const activities = get(freshStore);

      expect(activities).toEqual([]);
    });
  });

  describe('addActivity', () => {
    it('should add new activity to the beginning', () => {
      activityStore.addActivity('file_uploaded', 'test.txt');

      const activities = get(activityStore);
      expect(activities).toHaveLength(1);
      expect(activities[0].fileName).toBe('test.txt');
      expect(activities[0].type).toBe('file_uploaded');
    });

    it('should add activity with details', () => {
      activityStore.addActivity('file_renamed', 'new.txt', 'old.txt');

      const activities = get(activityStore);
      expect(activities[0].details).toBe('old.txt');
    });

    it('should maintain chronological order (newest first)', () => {
      activityStore.addActivity('file_uploaded', 'first.txt');
      activityStore.addActivity('file_uploaded', 'second.txt');
      activityStore.addActivity('file_uploaded', 'third.txt');

      const activities = get(activityStore);
      expect(activities[0].fileName).toBe('third.txt');
      expect(activities[1].fileName).toBe('second.txt');
      expect(activities[2].fileName).toBe('first.txt');
    });

    it('should limit to 50 activities', () => {
      // Add 60 activities
      for (let i = 0; i < 60; i++) {
        activityStore.addActivity('file_uploaded', `file-${i}.txt`);
      }

      const activities = get(activityStore);
      expect(activities).toHaveLength(50);
      expect(activities[0].fileName).toBe('file-59.txt'); // Most recent
      expect(activities[49].fileName).toBe('file-10.txt'); // 50th activity
    });

    it('should generate unique IDs', () => {
      activityStore.addActivity('file_uploaded', 'test.txt');
      activityStore.addActivity('file_uploaded', 'test.txt');

      const activities = get(activityStore);
      expect(activities[0].id).not.toBe(activities[1].id);
    });

    it('should save to localStorage', () => {
      activityStore.addActivity('file_uploaded', 'test.txt');

      const stored = localStorage.getItem('activity-history');
      expect(stored).toBeTruthy();

      const parsed = JSON.parse(stored!);
      expect(parsed).toHaveLength(1);
      expect(parsed[0].fileName).toBe('test.txt');
    });
  });

  describe('clearHistory', () => {
    it('should clear all activities', () => {
      activityStore.addActivity('file_uploaded', 'test1.txt');
      activityStore.addActivity('file_uploaded', 'test2.txt');

      activityStore.clearHistory();

      expect(get(activityStore)).toEqual([]);
    });

    it('should clear localStorage', () => {
      activityStore.addActivity('file_uploaded', 'test.txt');
      activityStore.clearHistory();

      const stored = localStorage.getItem('activity-history');
      const parsed = JSON.parse(stored!);
      expect(parsed).toEqual([]);
    });
  });

  describe('removeActivity', () => {
    it('should remove specific activity by ID', () => {
      activityStore.addActivity('file_uploaded', 'test1.txt');
      activityStore.addActivity('file_uploaded', 'test2.txt');

      const activities = get(activityStore);
      const idToRemove = activities[0].id;

      activityStore.removeActivity(idToRemove);

      const updated = get(activityStore);
      expect(updated).toHaveLength(1);
      expect(updated[0].id).not.toBe(idToRemove);
    });

    it('should update localStorage after removal', () => {
      activityStore.addActivity('file_uploaded', 'test1.txt');
      activityStore.addActivity('file_uploaded', 'test2.txt');

      const activities = get(activityStore);
      activityStore.removeActivity(activities[0].id);

      const stored = localStorage.getItem('activity-history');
      const parsed = JSON.parse(stored!);
      expect(parsed).toHaveLength(1);
    });

    it('should handle removing non-existent ID gracefully', () => {
      activityStore.addActivity('file_uploaded', 'test.txt');
      activityStore.removeActivity('non-existent-id');

      const activities = get(activityStore);
      expect(activities).toHaveLength(1);
    });
  });

  describe('getActivityDisplay', () => {
    const testCases: Array<{
      type: ActivityType;
      fileName: string;
      details?: string;
      expectedIcon: string;
      expectedTitle: string;
    }> = [
      {
        type: 'file_uploaded',
        fileName: 'test.txt',
        expectedIcon: '📤',
        expectedTitle: 'File Uploaded'
      },
      {
        type: 'file_downloaded',
        fileName: 'test.txt',
        expectedIcon: '📥',
        expectedTitle: 'File Downloaded'
      },
      {
        type: 'file_deleted',
        fileName: 'test.txt',
        expectedIcon: '🗑️',
        expectedTitle: 'File Deleted'
      },
      {
        type: 'file_renamed',
        fileName: 'new.txt',
        details: 'old.txt',
        expectedIcon: '✏️',
        expectedTitle: 'File Renamed'
      },
      {
        type: 'folder_created',
        fileName: 'Documents',
        expectedIcon: '📁',
        expectedTitle: 'Folder Created'
      },
      {
        type: 'share_created',
        fileName: 'test.txt',
        expectedIcon: '🔗',
        expectedTitle: 'Share Link Created'
      }
    ];

    testCases.forEach(({ type, fileName, details, expectedIcon, expectedTitle }) => {
      it(`should display correct info for ${type}`, () => {
        const activity: Activity = {
          id: '1',
          type,
          fileName,
          details,
          timestamp: new Date().toISOString()
        };

        const display = getActivityDisplay(activity);

        expect(display.icon).toBe(expectedIcon);
        expect(display.title).toBe(expectedTitle);
        expect(display.description).toBeTruthy();
      });
    });

    it('should include old name in rename description', () => {
      const activity: Activity = {
        id: '1',
        type: 'file_renamed',
        fileName: 'new.txt',
        details: 'old.txt',
        timestamp: new Date().toISOString()
      };

      const display = getActivityDisplay(activity);
      expect(display.description).toContain('old.txt');
      expect(display.description).toContain('new.txt');
    });
  });

  describe('getRelativeTime', () => {
    it('should show "Just now" for recent timestamps', () => {
      const now = new Date().toISOString();
      expect(getRelativeTime(now)).toBe('Just now');
    });

    it('should show minutes ago', () => {
      const fiveMinutesAgo = new Date(Date.now() - 5 * 60 * 1000).toISOString();
      expect(getRelativeTime(fiveMinutesAgo)).toBe('5m ago');
    });

    it('should show hours ago', () => {
      const threeHoursAgo = new Date(Date.now() - 3 * 60 * 60 * 1000).toISOString();
      expect(getRelativeTime(threeHoursAgo)).toBe('3h ago');
    });

    it('should show days ago', () => {
      const twoDaysAgo = new Date(Date.now() - 2 * 24 * 60 * 60 * 1000).toISOString();
      expect(getRelativeTime(twoDaysAgo)).toBe('2d ago');
    });

    it('should show formatted date for old timestamps', () => {
      const tenDaysAgo = new Date(Date.now() - 10 * 24 * 60 * 60 * 1000).toISOString();
      const result = getRelativeTime(tenDaysAgo);

      expect(result).toMatch(/\w{3} \d{1,2}/); // e.g., "Mar 9"
    });

    it('should handle edge cases', () => {
      const thirtySecondsAgo = new Date(Date.now() - 30 * 1000).toISOString();
      expect(getRelativeTime(thirtySecondsAgo)).toBe('Just now');

      const sixtyMinutesAgo = new Date(Date.now() - 60 * 60 * 1000).toISOString();
      expect(getRelativeTime(sixtyMinutesAgo)).toBe('1h ago');
    });
  });

  describe('Activity Types Coverage', () => {
    it('should support all activity types', () => {
      const types: ActivityType[] = [
        'file_uploaded',
        'file_downloaded',
        'file_deleted',
        'file_renamed',
        'file_moved',
        'folder_created',
        'folder_deleted',
        'folder_renamed',
        'share_created',
        'share_revoked'
      ];

      types.forEach((type) => {
        activityStore.addActivity(type, 'test.txt');
      });

      const activities = get(activityStore);
      expect(activities).toHaveLength(10);
    });
  });
});
