use std::fs;
use std::path::Path;

pub fn create_project(name: &str) {
    let project_dir = Path::new(name);

    if project_dir.exists() {
        eprintln!("Error: directory '{}' already exists", name);
        std::process::exit(1);
    }

    if let Err(e) = create_project_files(name, project_dir) {
        eprintln!("Error creating project '{}': {}", name, e);
        std::process::exit(1);
    }

    println!();
    println!("  Created new Forge project '{}'", name);
    println!();
    println!("  {}/", name);
    println!("    forge.toml");
    println!("    main.fg");
    println!("    tests/");
    println!("      basic_test.fg");
    println!("    .gitignore");
    println!();
    println!("  Get started:");
    println!("    cd {}", name);
    println!("    forge run main.fg");
    println!("    forge test");
    println!();
}

fn create_project_files(name: &str, project_dir: &Path) -> std::io::Result<()> {
    fs::create_dir_all(project_dir.join("tests"))?;

    let manifest = format!(
        r#"[project]
name = "{}"
version = "0.1.0"
description = ""
entry = "main.fg"

[dependencies]

[test]
directory = "tests"

[scripts]
dev = "forge run main.fg"
test = "forge test"
"#,
        name
    );
    fs::write(project_dir.join("forge.toml"), manifest)?;

    let main_fg = format!(
        r#"// {name} — a Forge project

say "Hello from {name}!"
"#,
        name = name
    );
    fs::write(project_dir.join("main.fg"), main_fg)?;

    let test_fg = r#"// Basic tests

@test
define should_work() {
    assert(1 + 1 == 2)
}
"#;
    fs::write(project_dir.join("tests/basic_test.fg"), test_fg)?;

    fs::write(
        project_dir.join(".gitignore"),
        "*.fgc\nforge_modules/\n.forge/\n",
    )?;

    Ok(())
}
