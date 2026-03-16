use crate::parser::ast::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Route {
    pub method: String,
    pub pattern: String,
    pub handler_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CorsMode {
    Restrictive,
    Permissive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    pub port: u16,
    pub host: String,
    pub cors: CorsMode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerPlan {
    pub config: ServerConfig,
    pub routes: Vec<Route>,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct SchedulePlan {
    pub interval: Expr,
    pub unit: String,
    pub body: Vec<Stmt>,
    pub line: usize,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct WatchPlan {
    pub path: Expr,
    pub body: Vec<Stmt>,
    pub line: usize,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Default)]
pub struct RuntimePlan {
    pub server: Option<ServerPlan>,
    pub schedules: Vec<SchedulePlan>,
    pub watches: Vec<WatchPlan>,
}

pub fn extract_runtime_plan(program: &Program) -> RuntimePlan {
    let mut server_config = None;
    let mut routes = Vec::new();
    let mut schedules = Vec::new();
    let mut watches = Vec::new();

    for spanned in &program.statements {
        match &spanned.stmt {
            Stmt::FnDef {
                name, decorators, ..
            } => {
                routes.extend(extract_routes(name, decorators));
            }
            Stmt::DecoratorStmt(dec) if dec.name == "server" && server_config.is_none() => {
                server_config = Some(extract_server_config(dec));
            }
            Stmt::ScheduleBlock {
                interval,
                unit,
                body,
            } => {
                schedules.push(SchedulePlan {
                    interval: interval.clone(),
                    unit: unit.clone(),
                    body: body.clone(),
                    line: spanned.line,
                });
            }
            Stmt::WatchBlock { path, body } => {
                watches.push(WatchPlan {
                    path: path.clone(),
                    body: body.clone(),
                    line: spanned.line,
                });
            }
            _ => {}
        }
    }

    RuntimePlan {
        server: server_config.map(|config| ServerPlan { config, routes }),
        schedules,
        watches,
    }
}

fn extract_routes(name: &str, decorators: &[Decorator]) -> Vec<Route> {
    let mut routes = Vec::new();
    for dec in decorators {
        let method = match dec.name.as_str() {
            "get" => "GET",
            "post" => "POST",
            "put" => "PUT",
            "delete" => "DELETE",
            "ws" => "WS",
            _ => continue,
        };
        let path = dec
            .args
            .iter()
            .find_map(|arg| match arg {
                DecoratorArg::Positional(Expr::StringLit(s)) => Some(s.clone()),
                _ => None,
            })
            .unwrap_or_else(|| format!("/{}", name));

        routes.push(Route {
            method: method.to_string(),
            pattern: path,
            handler_name: name.to_string(),
        });
    }
    routes
}

fn extract_server_config(decorator: &Decorator) -> ServerConfig {
    let mut config = ServerConfig {
        port: 8080,
        host: "127.0.0.1".to_string(),
        cors: CorsMode::Restrictive,
    };
    for arg in &decorator.args {
        match arg {
            DecoratorArg::Named(key, Expr::Int(n)) if key == "port" => config.port = *n as u16,
            DecoratorArg::Named(key, Expr::StringLit(s)) if key == "host" => {
                config.host = s.clone()
            }
            DecoratorArg::Named(key, Expr::StringLit(s)) if key == "cors" => {
                config.cors = match s.to_lowercase().as_str() {
                    "permissive" | "any" | "*" => CorsMode::Permissive,
                    _ => CorsMode::Restrictive,
                };
            }
            _ => {}
        }
    }
    config
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    fn parse_program(src: &str) -> Program {
        let tokens = Lexer::new(src).tokenize().expect("lex failed");
        Parser::new(tokens).parse_program().expect("parse failed")
    }

    #[test]
    fn extracts_server_runtime_plan() {
        let prog = parse_program(
            "@server(port: 3000, cors: \"permissive\")\n@get(\"/users/:id\") fn show(id) { return id }\n",
        );

        let plan = extract_runtime_plan(&prog);
        let server = plan.server.expect("server plan");
        assert_eq!(server.config.port, 3000);
        assert_eq!(server.config.host, "127.0.0.1");
        assert_eq!(server.config.cors, CorsMode::Permissive);
        assert_eq!(server.routes.len(), 1);
        assert_eq!(
            server.routes[0],
            Route {
                method: "GET".to_string(),
                pattern: "/users/:id".to_string(),
                handler_name: "show".to_string(),
            }
        );
    }

    #[test]
    fn server_route_defaults_to_function_name() {
        let prog = parse_program("@server\n@get fn list_users() { return [] }\n");

        let plan = extract_runtime_plan(&prog);
        let server = plan.server.expect("server plan");
        assert_eq!(server.routes.len(), 1);
        assert_eq!(server.routes[0].pattern, "/list_users");
    }

    #[test]
    fn first_server_decorator_wins() {
        let prog = parse_program("@server(port: 3000)\n@server(port: 9000)\n");

        let plan = extract_runtime_plan(&prog);
        let server = plan.server.expect("server plan");
        assert_eq!(server.config.port, 3000);
    }

    #[test]
    fn extracts_schedule_and_watch_metadata() {
        let prog = parse_program(
            "schedule every 5 minutes { let tick = 1 }\nwatch \"src/app.fg\" { let changed = true }\n",
        );

        let plan = extract_runtime_plan(&prog);
        assert_eq!(plan.schedules.len(), 1);
        assert_eq!(plan.schedules[0].unit, "minutes");
        assert_eq!(plan.schedules[0].line, 1);
        assert!(matches!(plan.schedules[0].interval, Expr::Int(5)));
        assert_eq!(plan.schedules[0].body.len(), 1);

        assert_eq!(plan.watches.len(), 1);
        assert_eq!(plan.watches[0].line, 2);
        assert!(matches!(plan.watches[0].path, Expr::StringLit(_)));
        assert_eq!(plan.watches[0].body.len(), 1);
    }

    #[test]
    fn routes_without_server_do_not_create_server_plan() {
        let prog = parse_program("@get(\"/users\") fn list_users() { return [] }\n");

        let plan = extract_runtime_plan(&prog);
        assert!(plan.server.is_none());
    }
}
