use bevy::{
    math::bounding::{Aabb2d, BoundingCircle, BoundingVolume, IntersectsVolume},
    prelude::*
};

const BACKGROUND_COLOR: Color = Color::srgb_u8(18, 18, 24);

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Paddle Force".into(),
                position: WindowPosition::Centered(MonitorSelection::Index(0)),
                mode: bevy::window::WindowMode::BorderlessFullscreen(MonitorSelection::Index(0)),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(Score(0))
        .insert_resource(ClearColor(BACKGROUND_COLOR))
        .add_systems(Startup, setup)
        // Add our gameplay simulation systems to the fixed timestep schedule
        // which runs at 64 Hz by default
        .add_systems(
            FixedUpdate,
            (apply_velocity, move_paddle, check_for_collisions)
                .chain(),
        )
        .add_systems(Update, (update_scoreboard, exit_on_escape))
        .run();
}

#[derive(Component)]
struct Paddle;

#[derive(Resource, Clone)]
struct PaddleInfo {
    // Using the default 2D camera they correspond 1:1 with screen pixels.
    size: Vec2,    
    speed: f32,
    // How close can the paddle get to the wall
    padding: f32,
    gap_to_floor: f32,
    color: Color,
}

fn insert_paddle_resource(commands: &mut Commands, _: &Single<&Window>) -> PaddleInfo {
    // TODO: update hardcoded values with window-scaled values.
    let paddle_info = PaddleInfo {
        size: Vec2::new(120.0, 20.0),
        speed: 500.0,
        padding: 10.0,
        gap_to_floor: 60.0,
        color: Color::srgb_u8(84, 160, 255),
    };

    commands.insert_resource(paddle_info.clone());

    paddle_info
}

#[derive(Component)]
struct Ball;

#[derive(Resource, Clone)]
struct BallInfo {
    starting_position: Vec3,
    initial_direction: Vec2,
    diameter: f32,
    speed: f32,
    color: Color,
}

fn insert_ball_resource(commands: &mut Commands, _: &Single<&Window>) -> BallInfo {
    // TODO: update hardcoded values with window-scaled values.

    let ball_info = BallInfo {
        // We set the z-value of the ball to 1 so it renders on top in the case of overlapping sprites.
        starting_position: Vec3::new(0.0, -50.0, 1.0),
        initial_direction: Vec2::new(0.5, -0.5),
        diameter: 30.0,
        speed: 400.0,
        color: Color::WHITE,
    };

    commands.insert_resource(ball_info.clone());

    ball_info
}

#[derive(Component, Deref, DerefMut)]
struct Velocity(Vec2);

#[derive(Event)]
struct BallCollided;

#[derive(Component)]
struct Brick;

#[derive(Resource, Clone)]
struct BrickInfo {
    size: Vec2,
    gap: f32,
    gap_to_paddle: f32,
    // These values are lower bounds, as the number of bricks is computed
    gap_to_ceiling: f32,
    gap_to_sides: f32,
    color: Color,
}

fn insert_brick_resource(commands: &mut Commands, _: &Single<&Window>) -> BrickInfo {
    // TODO: update hardcoded values with window-scaled values.

    let brick_info = BrickInfo {
        size: Vec2::new(100.0, 30.0),
        gap: 5.0,
        gap_to_paddle: 500.0,
        gap_to_ceiling: 20.0,
        gap_to_sides: 20.0,
        color: Color::srgb_u8(72, 219, 251),
    };

    commands.insert_resource(brick_info.clone());

    brick_info
}

// Default must be implemented to define this as a required component for the Wall component below
#[derive(Component, Default)]
struct Collider;

// This is a collection of the components that define a "Wall" in our game
#[derive(Component)]
#[require(Sprite, Transform, Collider)]
struct Wall;

#[derive(Resource, Clone)]
struct WallInfo {
    left: f32,
    right: f32,
    bottom: f32,
    top: f32,
    thickness: f32,
    color: Color,
}

fn insert_wall_resource(commands: &mut Commands, window: &Single<&Window>) -> WallInfo {
    let width = window.width();
    let left = -width / 2.0 + 50.0;
    let right = width / 2.0 - 50.0;

    let height = window.height();
    let bottom = -height / 2.0 + 100.0;
    let top = height / 2.0 - 100.0;

    let wall_info = WallInfo {
        left,
        right,
        bottom,
        top,
        thickness: 10.0,
        color: Color::srgb(0.8, 0.8, 0.8),
    };
    commands.insert_resource(wall_info.clone());

    wall_info
}

/// Which side of the arena is this wall located on?
enum WallLocation {
    Left,
    Right,
    Bottom,
    Top,
}

impl WallLocation {
    /// Location of the *center* of the wall, used in `transform.translation()`
    fn position(&self, wall: &WallInfo) -> Vec2 {
        match self {
            WallLocation::Left => Vec2::new(wall.left, 0.),
            WallLocation::Right => Vec2::new(wall.right, 0.),
            WallLocation::Bottom => Vec2::new(0., wall.bottom),
            WallLocation::Top => Vec2::new(0., wall.top),
        }
    }

    /// (x, y) dimensions of the wall, used in `transform.scale()`
    fn size(&self, wall: &WallInfo) -> Vec2 {
        let arena_height = wall.top - wall.bottom;
        let arena_width = wall.right - wall.left;
        // Make sure we haven't messed up our constants
        assert!(arena_height > 0.0);
        assert!(arena_width > 0.0);

        match self {
            WallLocation::Left | WallLocation::Right => {
                Vec2::new(wall.thickness, arena_height + wall.thickness)
            }
            WallLocation::Bottom | WallLocation::Top => {
                Vec2::new(arena_width + wall.thickness, wall.thickness)
            }
        }
    }
}

impl Wall {
    // This "builder method" allows us to reuse logic across our wall entities,
    // making our code easier to read and less prone to bugs when we change the logic
    // Notice the use of Sprite and Transform alongside Wall, overwriting the default values defined for the required components
    fn new(location: WallLocation, wall: &WallInfo) -> (Wall, Sprite, Transform) {
        (
            Wall,
            Sprite::from_color(wall.color, Vec2::ONE),
            Transform {
                // We need to convert our Vec2 into a Vec3, by giving it a z-coordinate
                // This is used to determine the order of our sprites
                translation: location.position(wall).extend(0.0),
                // The z-scale of 2D objects must always be 1.0,
                // or their ordering will be affected in surprising ways.
                // See https://github.com/bevyengine/bevy/issues/4149
                scale: location.size(wall).extend(1.0),
                ..default()
            },
        )
    }
}

// This resource tracks the game's score
#[derive(Resource, Deref, DerefMut)]
struct Score(usize);

#[derive(Resource, Clone)]
struct ScoreInfo {
    font_size: f32,
    text_padding: Val,
    text_color: Color,
    score_color: Color,
}

fn insert_score_resource(commands: &mut Commands) -> ScoreInfo {
    let score_info = ScoreInfo {
        font_size: 33.0,
        text_padding: Val::Px(5.0),
        text_color: Color::srgb_u8(230, 230, 235),
        score_color: Color::srgb_u8(80, 160, 255),
    };

    commands.insert_resource(score_info.clone());

    score_info
}

#[derive(Component)]
struct ScoreboardUi;

#[derive(Resource, Clone)]
struct WindowInfo {
    width: f32,
    height: f32,
}

fn insert_window_resource(commands: &mut Commands, window: &Single<&Window>) -> WindowInfo {
    let width = window.width();
    let height = window.height();

    let window_info = WindowInfo {
        width,
        height,
    };

    commands.insert_resource(window_info.clone());

    window_info
}

// Add the game's entities to our world
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    _: Res<AssetServer>,
    window: Single<&Window>
) {
    // Camera
    commands.spawn(Camera2d);

    insert_window_resource(&mut commands, &window);

    let wall_info = insert_wall_resource(&mut commands, &window);

    // Paddle
    let paddle_info = insert_paddle_resource(&mut commands, &window);

    let paddle_y = wall_info.bottom + paddle_info.gap_to_floor;

    commands.spawn((
        Sprite::from_color(paddle_info.color, Vec2::ONE),
        Transform {
            translation: Vec3::new(0.0, paddle_y, 0.0),
            scale: paddle_info.size.extend(1.0),
            ..default()
        },
        Paddle,
        Collider,
    ));

    // Ball
    let ball_info = insert_ball_resource(&mut commands, &window);

    commands.spawn((
        Mesh2d(meshes.add(Circle::default())),
        MeshMaterial2d(materials.add(ball_info.color)),
        Transform::from_translation(ball_info.starting_position)
            .with_scale(Vec2::splat(ball_info.diameter).extend(1.)),
        Ball,
        Velocity(ball_info.initial_direction.normalize() * ball_info.speed),
    ));

    // Scoreboard
    let score_info = insert_score_resource(&mut commands);

    commands.spawn((
        Text::new("Score: "),
        TextFont {
            font_size: score_info.font_size,
            ..default()
        },
        TextColor(score_info.text_color),
        ScoreboardUi,
        Node {
            position_type: PositionType::Absolute,
            top: score_info.text_padding,
            left: score_info.text_padding,
            ..default()
        },
        children![(
            TextSpan::default(),
            TextFont {
                font_size: score_info.font_size,
                ..default()
            },
            TextColor(score_info.score_color),
        )],
    ));

    // Walls
    commands.spawn(Wall::new(WallLocation::Left, &wall_info));
    commands.spawn(Wall::new(WallLocation::Right, &wall_info));
    commands.spawn(Wall::new(WallLocation::Bottom, &wall_info));
    commands.spawn(Wall::new(WallLocation::Top, &wall_info));

    // Bricks
    let brick_info = insert_brick_resource(&mut commands, &window);

    let total_width_of_bricks = (wall_info.right - wall_info.left) - 2. * brick_info.gap_to_sides;
    let bottom_edge_of_bricks = paddle_y + brick_info.gap_to_paddle;
    let total_height_of_bricks = wall_info.top - bottom_edge_of_bricks - brick_info.gap_to_ceiling;

    assert!(total_width_of_bricks > 0.0);
    assert!(total_height_of_bricks > 0.0);

    // Given the space available, compute how many rows and columns of bricks we can fit
    let n_columns = (total_width_of_bricks / (brick_info.size.x + brick_info.gap)).floor() as usize;
    let n_rows = (total_height_of_bricks / (brick_info.size.y + brick_info.gap)).floor() as usize;
    let n_vertical_gaps = n_columns - 1;

    // Because we need to round the number of columns,
    // the space on the top and sides of the bricks only captures a lower bound, not an exact value
    let center_of_bricks = (wall_info.left + wall_info.right) / 2.0;
    let left_edge_of_bricks = center_of_bricks
        // Space taken up by the bricks
        - (n_columns as f32 / 2.0 * brick_info.size.x)
        // Space taken up by the gaps
        - n_vertical_gaps as f32 / 2.0 * brick_info.gap;

    // In Bevy, the `translation` of an entity describes the center point,
    // not its bottom-left corner
    let offset_x = left_edge_of_bricks + brick_info.size.x / 2.;
    let offset_y = bottom_edge_of_bricks + brick_info.size.y / 2.;

    for row in 0..n_rows {
        for column in 0..n_columns {
            let brick_position = Vec2::new(
                offset_x + column as f32 * (brick_info.size.x + brick_info.gap),
                offset_y + row as f32 * (brick_info.size.y + brick_info.gap),
            );

            // brick
            commands.spawn((
                Sprite {
                    color: brick_info.color,
                    ..default()
                },
                Transform {
                    translation: brick_position.extend(0.0),
                    scale: Vec3::new(brick_info.size.x, brick_info.size.y, 1.0),
                    ..default()
                },
                Brick,
                Collider,
            ));
        }
    }
}

fn move_paddle(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut paddle_transform: Single<&mut Transform, With<Paddle>>,
    time: Res<Time>,
    wall: Res<WallInfo>,
    paddle: Res<PaddleInfo>,
) {
    let mut direction = 0.0;

    if keyboard_input.pressed(KeyCode::ArrowLeft) {
        direction -= 1.0;
    }

    if keyboard_input.pressed(KeyCode::ArrowRight) {
        direction += 1.0;
    }

    // Calculate the new horizontal paddle position based on player input
    let new_paddle_position =
        paddle_transform.translation.x + direction * paddle.speed * time.delta_secs();

    // Update the paddle position,
    // making sure it doesn't cause the paddle to leave the arena
    let left_bound = wall.left + wall.thickness / 2.0 + paddle.size.x / 2.0 + paddle.padding;
    let right_bound = wall.right - wall.thickness / 2.0 - paddle.size.x / 2.0 - paddle.padding;

    paddle_transform.translation.x = new_paddle_position.clamp(left_bound, right_bound);
}

fn apply_velocity(mut query: Query<(&mut Transform, &Velocity)>, time: Res<Time>) {
    for (mut transform, velocity) in &mut query {
        transform.translation.x += velocity.x * time.delta_secs();
        transform.translation.y += velocity.y * time.delta_secs();
    }
}

fn update_scoreboard(
    score: Res<Score>,
    score_root: Single<Entity, (With<ScoreboardUi>, With<Text>)>,
    mut writer: TextUiWriter,
) {
    *writer.text(*score_root, 1) = score.to_string();
}

fn check_for_collisions(
    mut commands: Commands,
    mut score: ResMut<Score>,
    ball: Res<BallInfo>,
    ball_query: Single<(&mut Velocity, &Transform), With<Ball>>,
    collider_query: Query<(Entity, &Transform, Option<&Brick>), With<Collider>>,
) {
    let (mut ball_velocity, ball_transform) = ball_query.into_inner();

    for (collider_entity, collider_transform, maybe_brick) in &collider_query {
        let collision = ball_collision(
            BoundingCircle::new(ball_transform.translation.truncate(), ball.diameter / 2.0),
            Aabb2d::new(
                collider_transform.translation.truncate(),
                collider_transform.scale.truncate() / 2.0,
            ),
        );

        if let Some(collision) = collision {
            // Trigger observers of the "BallCollided" event
            commands.trigger(BallCollided);

            // Bricks should be despawned and increment the scoreboard on collision
            if maybe_brick.is_some() {
                commands.entity(collider_entity).despawn();
                **score += 1;
            }

            // Reflect the ball's velocity when it collides
            let mut reflect_x = false;
            let mut reflect_y = false;

            // Reflect only if the velocity is in the opposite direction of the collision
            // This prevents the ball from getting stuck inside the bar
            match collision {
                Collision::Left => reflect_x = ball_velocity.x > 0.0,
                Collision::Right => reflect_x = ball_velocity.x < 0.0,
                Collision::Top => reflect_y = ball_velocity.y < 0.0,
                Collision::Bottom => reflect_y = ball_velocity.y > 0.0,
            }

            // Reflect velocity on the x-axis if we hit something on the x-axis
            if reflect_x {
                ball_velocity.x = -ball_velocity.x;
            }

            // Reflect velocity on the y-axis if we hit something on the y-axis
            if reflect_y {
                ball_velocity.y = -ball_velocity.y;
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum Collision {
    Left,
    Right,
    Top,
    Bottom,
}

// Returns `Some` if `ball` collides with `bounding_box`.
// The returned `Collision` is the side of `bounding_box` that `ball` hit.
fn ball_collision(ball: BoundingCircle, bounding_box: Aabb2d) -> Option<Collision> {
    if !ball.intersects(&bounding_box) {
        return None;
    }

    let closest = bounding_box.closest_point(ball.center());
    let offset = ball.center() - closest;
    let side = if offset.x.abs() > offset.y.abs() {
        if offset.x < 0. {
            Collision::Left
        } else {
            Collision::Right
        }
    } else if offset.y > 0. {
        Collision::Top
    } else {
        Collision::Bottom
    };

    Some(side)
}

fn exit_on_escape(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut exit: MessageWriter<AppExit>
) {
    if keyboard_input.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
}
