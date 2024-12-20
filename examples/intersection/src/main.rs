use bevy::{
    prelude::*,
    render::{
        mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
        render_asset::RenderAssetUsages,
    },
};
use bevy_egui::{egui, EguiContexts, EguiPlugin};
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin};
use cvoxel::CVoxels;
use nalgebra::{Point3, UnitQuaternion, Vector3};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(EguiPlugin)
        .add_plugins(PanOrbitCameraPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, update_from_cvoxel_transform)
        .add_systems(Update, draw_voxel_aabb)
        .add_systems(Update, draw_intersection_aabb)
        .add_systems(Update, draw_intersecting_voxel)
        .add_systems(Update, ui)
        .run();
}

#[derive(Resource)]
struct AppSettings {
    show_bounding_box: bool,
}

#[derive(Component)]
struct CVoxelComponent {
    inner: CVoxels,
}

fn isometry_scale_to_transform(
    isometry: &nalgebra::Isometry3<f32>,
    scale: &Vector3<f32>,
) -> Transform {
    let mut transform = Transform::IDENTITY;
    transform.translation = Vec3::from_array(isometry.translation.vector.data.0[0]);
    transform.rotation = Quat::from_array(isometry.rotation.quaternion().coords.data.0[0]);
    transform.scale = Vec3::from_array(scale.data.0[0]);
    transform
}

fn update_from_cvoxel_transform(mut query: Query<(&CVoxelComponent, &mut Transform)>) {
    for (cvoxels, mut transform) in query.iter_mut() {
        transform.translation =
            Vec3::from_array(cvoxels.inner.transform.translation.vector.data.0[0]);
        transform.rotation =
            Quat::from_array(cvoxels.inner.transform.rotation.quaternion().coords.data.0[0]);
    }
}

fn draw_voxel_aabb(
    settings: Res<AppSettings>,
    voxels: Query<&CVoxelComponent>,
    mut gizmos: Gizmos,
) {
    if !settings.show_bounding_box {
        return;
    }
    for cvoxels in voxels.iter() {
        let transform =
            isometry_scale_to_transform(&cvoxels.inner.transform, &(cvoxels.inner.half_size * 2.0));
        gizmos.cuboid(transform, Color::linear_rgb(0.0, 1.0, 0.0));
    }
}

fn draw_intersection_aabb(voxels: Query<&CVoxelComponent>, mut gizmos: Gizmos) {
    for (i, ci) in voxels.iter().enumerate() {
        for (j, cj) in voxels.iter().enumerate() {
            if i == j {
                continue;
            }
            if let Some(aabb) = ci.inner.intersection_aabb(&cj.inner) {
                let translation = aabb.middle().coords;
                let scale = aabb.size();
                let mut isometry = nalgebra::Isometry3::identity();
                isometry.translation = translation.into();
                isometry = ci.inner.transform * isometry;
                let transform = isometry_scale_to_transform(&isometry, &scale);
                gizmos.cuboid(transform, Color::linear_rgb(1.0, 1.0, 0.0));
            }
        }
    }
}

fn draw_single_voxel_in_object(index: usize, cvoxel: &CVoxels, gizmos: &mut Gizmos) {
    let z = index / cvoxel.area;
    let left = index % cvoxel.area;
    let y = left / cvoxel.shape.x;
    let x = left % cvoxel.shape.x;
    let voxel_size = Vector3::new(cvoxel.dx, cvoxel.dx, cvoxel.dx);
    let coords =
        Point3::new(x, y, z).cast::<f32>() * cvoxel.dx + voxel_size * 0.5 - cvoxel.half_size;

    let mut isometry = nalgebra::Isometry3::identity();
    isometry.translation = coords.into();
    isometry = cvoxel.transform * isometry;
    let transform = isometry_scale_to_transform(&isometry, &voxel_size);

    gizmos.cuboid(transform, Color::linear_rgb(1.0, 0.0, 0.0));
}

fn draw_intersecting_voxel(voxels: Query<&CVoxelComponent>, mut gizmos: Gizmos) {
    for (i, ci) in voxels.iter().enumerate() {
        for (j, cj) in voxels.iter().enumerate() {
            if i == j {
                continue;
            }
            if let Some((i1, i2)) = ci.inner.intersected(&cj.inner) {
                draw_single_voxel_in_object(i1, &ci.inner, &mut gizmos);
                draw_single_voxel_in_object(i2, &cj.inner, &mut gizmos);
            }
        }
    }
}

fn voxelize_mesh(mesh: &Mesh, dx: f32) -> Option<CVoxels> {
    let mesh_attr = mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap();
    if let VertexAttributeValues::Float32x3(v) = mesh_attr {
        if let Some(indices) = mesh.indices() {
            match indices {
                Indices::U16(ids) => CVoxels::from_indexed_mesh(&v, &ids, dx),
                Indices::U32(ids) => CVoxels::from_indexed_mesh(&v, &ids, dx),
            }
        } else {
            CVoxels::from_trimesh(&v, dx)
        }
    } else {
        None
    }
}

fn cvoxel_surface_mesh(voxels: &CVoxels) -> Mesh {
    let surface_mesh = voxels.surface_mesh();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, surface_mesh.position)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, surface_mesh.color)
    .with_computed_normals()
}

fn ui(
    mut voxels: Query<(&mut CVoxelComponent, &mut Visibility)>,
    mut contexts: EguiContexts,
    mut panorbit: Query<&mut PanOrbitCamera>,
    settings: ResMut<AppSettings>,
) {
    let settings = settings.into_inner();
    let response = egui::Window::new("Voxel Objects").show(contexts.ctx_mut(), |ui| {
        // visualization
        ui.checkbox(&mut settings.show_bounding_box, "Show Bounding Box");

        // controls
        for (i, (mut cvoxel, visibility)) in voxels.iter_mut().enumerate() {
            let transform = &mut cvoxel.inner.transform;

            let speed = 0.01;

            let visibility = visibility.into_inner();
            egui::ComboBox::from_id_salt(i)
                .selected_text(format!("{:?}", visibility))
                .show_ui(ui, |ui| {
                    ui.selectable_value(visibility, Visibility::Inherited, "Inherited");
                    ui.selectable_value(visibility, Visibility::Hidden, "Hidden");
                    ui.selectable_value(visibility, Visibility::Visible, "Visible");
                });

            let mut euler = transform.rotation.euler_angles();
            ui.label("Euler:");

            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut euler.0)
                        .prefix("roll: ")
                        .speed(speed),
                );
                ui.add(
                    egui::DragValue::new(&mut euler.1)
                        .prefix("pitch: ")
                        .speed(speed),
                );
                ui.add(
                    egui::DragValue::new(&mut euler.2)
                        .prefix("yaw: ")
                        .speed(speed),
                );
            });
            transform.rotation = UnitQuaternion::from_euler_angles(euler.0, euler.1, euler.2);

            let pos = &mut transform.translation.vector.data.0[0];
            ui.label("Pos:");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut pos[0]).prefix("x: ").speed(speed));
                ui.add(egui::DragValue::new(&mut pos[1]).prefix("y: ").speed(speed));
                ui.add(egui::DragValue::new(&mut pos[2]).prefix("z: ").speed(speed));
            });

            ui.separator();
        }
    });
    let mut panorbit = panorbit.single_mut();
    if let Some(inner) = response {
        let response = inner.response;

        if response.ctx.is_using_pointer() {
            panorbit.enabled = false;
        } else {
            panorbit.enabled = true;
        }
    } else {
        panorbit.enabled = true;
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // settings
    commands.insert_resource(AppSettings {
        show_bounding_box: true,
    });

    // camera
    commands.spawn((
        Camera3dBundle {
            transform: Transform::from_xyz(0.0, 1.0, 2.0)
                .looking_at(Vec3::new(0.0, 0.0, 0.0), Dir3::Y),
            ..Default::default()
        },
        PanOrbitCamera::default(),
    ));

    // light
    commands.spawn(PointLightBundle {
        point_light: PointLight {
            intensity: 10000.0,
            color: Color::WHITE,
            ..Default::default()
        },
        transform: Transform::from_xyz(0.0, 1.0, 2.0),
        ..Default::default()
    });

    // meshes
    let shapes = [
        Capsule3d::new(0.3, 0.7).mesh().build(),
        Sphere::new(0.3).mesh().build(),
        Torus::new(0.2, 0.5).mesh().build(),
    ];

    // voxel objects
    let dx = 0.05;
    for i in 0..shapes.len() {
        let mesh = &shapes[i];
        let mut cvoxel = voxelize_mesh(mesh, dx).unwrap();
        cvoxel.transform.translation.x = (i as f32 + 0.5 - shapes.len() as f32 * 0.5) * 0.9;
        let surface_mesh = cvoxel_surface_mesh(&cvoxel);
        commands.spawn((
            PbrBundle {
                mesh: meshes.add(surface_mesh),
                material: materials.add(Color::WHITE),
                ..Default::default()
            },
            CVoxelComponent { inner: cvoxel },
        ));
    }
}
