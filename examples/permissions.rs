use logify::{Expression, ExpressionBuilder, eval::BoolEval};

fn main() -> Result<(), ()> {
    // 1. Build the permission rules
    let builder = ExpressionBuilder::new();

    // Define Roles (Leaves)
    let admin = builder.leaf("Admin");
    let editor = builder.leaf("Editor");
    let viewer = builder.leaf("Viewer");

    // Define Flags (Leaves)
    let banned = builder.leaf("Banned");
    let readonly = builder.leaf("ReadOnly");

    // Construct Logic
    let can_view = (admin | editor | viewer) & !banned;
    let can_edit = (admin | editor) & !readonly & !banned;
    let can_delete = admin & !banned;

    // Add roots in specific order (0=View, 1=Edit, 2=Delete)
    builder.add_root(can_view);
    builder.add_root(can_edit);
    builder.add_root(can_delete);

    let rules: Expression<&str> = builder.build();

    // 2. Define users

    // User A: A Banned Editor (Should have NO access)
    let mut user_a = BoolEval::new();
    user_a.add("Editor");
    user_a.add("Banned");

    // User B: A Read-Only Editor (Can View, Cannot Edit)
    let mut user_b = BoolEval::new();
    user_b.add("Editor");
    user_b.add("ReadOnly");

    // User C: An Admin (Can do everything)
    let mut user_c = BoolEval::new();
    user_c.add("Admin");

    // 3. Evaluate
    print_access("User A (Banned Editor)", &rules, &mut user_a);
    print_access("User B (ReadOnly Editor)", &rules, &mut user_b);
    print_access("User C (Admin)", &rules, &mut user_c);

    Ok(())
}

fn print_access<'a>(name: &str, rules: &Expression<&'a str>, user: &mut BoolEval<&'a str>) {
    // Get the roots (Each representing a permission)
    let perms = rules.evaluate(user).unwrap();

    println!("Permissions for {}:", name);
    println!("  [View]:   {}", perms[0]);
    println!("  [Edit]:   {}", perms[1]);
    println!("  [Delete]: {}", perms[2]);
    println!("-----------------------------");
}
