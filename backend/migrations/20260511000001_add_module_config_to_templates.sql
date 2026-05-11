-- Add module_config to templates for module-specific initialization data
ALTER TABLE templates ADD COLUMN module_config JSONB NOT NULL DEFAULT '{}';

-- Backfill existing system kanban templates with current standard_columns config
UPDATE templates
SET module_config = jsonb_build_object(
    'kanban', jsonb_build_object(
        'columns', jsonb_build_array(
            jsonb_build_object('id','column_backlog','title','Backlog','slug','00-Backlog','order',0,'status','backlog'),
            jsonb_build_object('id','column_ready','title','Ready','slug','01-Ready','order',1,'status','ready'),
            jsonb_build_object('id','column_in_progress','title','In Progress','slug','02-In-Progress','order',2,'status','in_progress'),
            jsonb_build_object('id','column_review','title','Review','slug','03-Review','order',3,'status','review'),
            jsonb_build_object('id','column_done','title','Done','slug','04-Done','order',4,'status','done')
        ),
        'labels', jsonb_build_array(
            jsonb_build_object('id','label_green','name','Low','color','green'),
            jsonb_build_object('id','label_yellow','name','Medium','color','yellow'),
            jsonb_build_object('id','label_orange','name','High','color','orange'),
            jsonb_build_object('id','label_red','name','Urgent','color','red')
        ),
        'settings', jsonb_build_object(
            'show_description_on_cards', true,
            'description_preview_lines', 2,
            'show_assignees', true,
            'show_labels', true,
            'show_due_date', true,
            'show_attachment_badge', true,
            'show_checklist_badge', true
        )
    )
)
WHERE module_key = 'kanban' AND system_template = true;
