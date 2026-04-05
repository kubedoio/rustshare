import { describe, it, expect } from 'vitest';
import { extractFolderPaths, sortFolderPaths } from './directoryUpload';

describe('directoryUpload utils', () => {
  describe('extractFolderPaths', () => {
    it('should extract all folder paths from relative paths', () => {
      const items = [
        { file: new File([], 'a.ts'), relativePath: 'src/components/a.ts' },
        { file: new File([], 'b.ts'), relativePath: 'src/utils/b.ts' },
        { file: new File([], 'c.txt'), relativePath: 'c.txt' }
      ];
      const result = extractFolderPaths(items);
      expect(result).toContain('src');
      expect(result).toContain('src/components');
      expect(result).toContain('src/utils');
      expect(result).toHaveLength(3);
    });

    it('should return empty array when all files are root-level', () => {
      const items = [
        { file: new File([], 'a.txt'), relativePath: 'a.txt' },
        { file: new File([], 'b.txt'), relativePath: 'b.txt' }
      ];
      const result = extractFolderPaths(items);
      expect(result).toHaveLength(0);
    });
  });

  describe('sortFolderPaths', () => {
    it('should sort paths by depth so parents are created first', () => {
      const paths = ['a/b/c', 'a', 'a/b'];
      const result = sortFolderPaths(paths);
      expect(result).toEqual(['a', 'a/b', 'a/b/c']);
    });

    it('should keep same-depth paths in stable order', () => {
      const paths = ['z', 'a'];
      const result = sortFolderPaths(paths);
      expect(result).toEqual(['z', 'a']);
    });
  });
});
