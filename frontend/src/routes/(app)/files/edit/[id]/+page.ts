import type { PageLoad } from './$types';
import { redirect } from '@sveltejs/kit';

export const load: PageLoad = async ({ params, fetch }) => {
  const fileId = params.id;
  
  // Fetch file metadata
  const response = await fetch(`/api/v1/files/${fileId}`);
  
  if (!response.ok) {
    throw redirect(302, '/files');
  }
  
  const file = await response.json();
  
  // Verify it's an image
  if (!file.mime_type?.startsWith('image/')) {
    throw redirect(302, '/files');
  }
  
  return {
    file,
    fileId
  };
};
