use bevy::{prelude::*, text::BreakLineOn, window::PrimaryWindow};
use serde::{Deserialize, Serialize};

use std::{collections::VecDeque, path::PathBuf};
use uuid::Uuid;

#[path = "ui_helpers/ui_helpers.rs"]
mod ui_helpers;
use ui_helpers::*;
#[path = "systems/save.rs"]
mod save_systems;
use save_systems::*;
#[path = "systems/load.rs"]
mod load_systems;
use load_systems::*;
#[path = "systems/keyboard.rs"]
mod keyboard_systems;
use keyboard_systems::*;
#[path = "systems/path_modal.rs"]
mod path_modal_systems;
use path_modal_systems::*;
#[path = "systems/init_layout.rs"]
mod init_layout;
use init_layout::*;
#[path = "systems/resize.rs"]
mod resize;
use resize::*;
#[path = "systems/arrows.rs"]
mod arrows;
use arrows::*;
#[path = "systems/button_handlers.rs"]
mod button_handlers;
use button_handlers::*;
#[path = "systems/tabs.rs"]
mod tabs;
use tabs::*;

pub struct ChartPlugin;

pub struct AddRect {
    pub node: JsonNode,
    pub image: Option<UiImage>,
}

pub struct SetWindowIcon {
    pub image: Handle<Image>,
}

pub struct RedrawArrow {
    pub id: ReflectableUuid,
}

#[derive(Component)]
pub struct MainCamera;

#[derive(Resource, Debug)]
pub struct SaveRequest {
    pub path: Option<PathBuf>,
    pub tab_id: Option<ReflectableUuid>, // None means save to active tab
}

#[derive(Resource, Debug)]
pub struct LoadRequest {
    pub path: Option<PathBuf>,
    pub drop_last_checkpoint: bool, // Useful for undo functionality
}

#[derive(Serialize, Deserialize)]
pub enum NodeType {
    Rect,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum TextPos {
    Center,
    BottomRight,
    BottomLeft,
    TopRight,
    TopLeft,
}

#[derive(Serialize, Deserialize)]
pub struct JsonNodeText {
    pub text: String,
    pub pos: TextPos,
}

#[derive(Serialize, Deserialize)]
pub struct JsonNode {
    pub id: Uuid,
    pub node_type: NodeType,
    pub left: Val,
    pub bottom: Val,
    pub width: Val,
    pub height: Val,
    pub text: JsonNodeText,
    pub bg_color: Color,
    pub tags: Vec<String>,
    pub z_index: i32,
}

#[derive(Serialize, Deserialize)]
pub struct Tab {
    pub is_active: bool,
    pub id: ReflectableUuid,
    pub name: String,
    pub checkpoints: VecDeque<String>,
}

#[derive(Resource, Default)]
pub struct AppState {
    pub path_modal_id: Option<ReflectableUuid>,
    pub main_panel: Option<Entity>,
    pub arrow_type: ArrowType,
    pub entity_to_edit: Option<ReflectableUuid>,
    pub tab_to_edit: Option<ReflectableUuid>,
    pub hold_entity: Option<ReflectableUuid>,
    pub entity_to_resize: Option<(ReflectableUuid, ResizeMarker)>,
    pub arrow_to_draw_start: Option<ArrowConnect>,
    pub tabs: Vec<Tab>,
}

impl Plugin for ChartPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AppState>();

        app.register_type::<Rectangle>();
        app.register_type::<EditableText>();
        app.register_type::<ArrowConnect>();
        app.register_type::<ResizeMarker>();
        app.register_type::<ReflectableUuid>();
        app.register_type_data::<ReflectableUuid, ReflectSerialize>();
        app.register_type_data::<ReflectableUuid, ReflectDeserialize>();
        app.register_type::<ArrowConnectPos>();

        app.register_type::<BreakLineOn>();

        app.add_event::<AddRect>();
        app.add_event::<SetWindowIcon>();
        app.add_event::<CreateArrow>();
        app.add_event::<RedrawArrow>();

        app.add_startup_system(init_layout);

        app.add_systems((
            button_handler,
            update_rectangle_position,
            create_new_rectangle,
            resize_entity_start,
            resize_entity_end,
            create_arrow_start,
            create_arrow_end,
            set_focused_entity,
            redraw_arrows,
            keyboard_input_system,
            cancel_path_modal,
            path_modal_keyboard_input_system,
            set_focused_modal,
            confirm_path_modal,
            open_path_modal,
        ));

        app.add_systems(
            (save_json, remove_save_request)
                .chain()
                .distributive_run_if(should_save),
        );

        app.add_systems(
            (load_json, remove_load_request)
                .chain()
                .distributive_run_if(should_load),
        );

        app.add_systems((
            change_color_pallete,
            change_arrow_type,
            change_text_pos,
            add_tab_handler,
            delete_tab_handler,
            selected_tab_handler,
            rename_tab_handler,
            tab_keyboard_input_system,
        ));
    }
}

fn set_focused_entity(
    mut interaction_query: Query<
        (&Interaction, &Rectangle),
        (Changed<Interaction>, With<Rectangle>),
    >,
    mut state: ResMut<AppState>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    buttons: Res<Input<MouseButton>>,
) {
    let mut window = windows.single_mut();
    for (interaction, rectangle) in &mut interaction_query {
        match *interaction {
            Interaction::Clicked => {
                window.cursor.icon = CursorIcon::Text;
                state.hold_entity = Some(rectangle.id);
                state.entity_to_edit = Some(rectangle.id);
            }
            Interaction::Hovered => {
                if state.hold_entity.is_none() {
                    window.cursor.icon = CursorIcon::Move;
                }
            }
            Interaction::None => {
                window.cursor.icon = CursorIcon::Default;
            }
        }
    }
    if buttons.just_released(MouseButton::Left) {
        state.hold_entity = None;
        state.entity_to_resize = None;
    }
}

fn update_rectangle_position(
    mut cursor_moved_events: EventReader<CursorMoved>,
    mut node_position: Query<(&mut Style, &Rectangle), With<Rectangle>>,
    state: Res<AppState>,
    mut query: Query<(&Style, &LeftPanel), Without<Rectangle>>,
    mut events: EventWriter<RedrawArrow>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let primary_window = windows.single();
    for event in cursor_moved_events.iter() {
        for (mut style, top) in &mut node_position.iter_mut() {
            if Some(top.id) == state.hold_entity {
                let size = query.single_mut().0.size;
                if let (Val::Percent(x), Val::Px(element_width)) = (size.width, style.size.width) {
                    let width = (primary_window.width() * x) / 100.;
                    style.position.left = Val::Px(event.position.x - width - element_width / 2.);
                }
                if let Val::Px(element_height) = style.size.height {
                    style.position.bottom = Val::Px(event.position.y - element_height / 2.);
                }
                events.send(RedrawArrow { id: top.id });
            }
        }
    }
}

fn create_new_rectangle(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut events: EventReader<AddRect>,
    mut state: ResMut<AppState>,
) {
    for event in events.iter() {
        let font = asset_server.load("fonts/iosevka-regular.ttf");
        state.entity_to_edit = Some(ReflectableUuid(event.node.id));
        let entity = spawn_node(
            &mut commands,
            NodeMeta {
                font,
                size: (event.node.width, event.node.height),
                id: ReflectableUuid(event.node.id),
                image: event.image.clone(),
                text: event.node.text.text.clone(),
                bg_color: event.node.bg_color,
                position: (event.node.left, event.node.bottom),
                text_pos: event.node.text.pos.clone(),
                tags: event.node.tags.clone(),
                z_index: event.node.z_index,
            },
        );
        commands.entity(state.main_panel.unwrap()).add_child(entity);
    }
}
