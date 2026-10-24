"""Regression cases for the bounded Rust SQL guard (no Rust build or DB needed)."""

import contextlib
import io
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import check_handler_sql as guard


class HandlerSqlGuardTests(unittest.TestCase):
    def test_each_sql_form_is_detected_without_other_imports(self):
        cases = [
            ('sqlx::query("SELECT 1");', "query"),
            ('::sqlx::query_scalar::<_, i64>("SELECT 1");', "query_scalar"),
            ('sqlx::r#query("SELECT 1");', "query"),
            ('sqlx::query!("SELECT 1");', "query"),
            ('sqlx::query! { "SELECT 1" };', "query"),
            ('sqlx::query! [ "SELECT 1" ];', "query"),
            ('sqlx::query_file_as!(Row, "query.sql");', "query_file_as"),
            ('use ::sqlx::query as q; q("SELECT 1");', "query"),
            ('use sqlx::{query as q}; q("SELECT 1");', "query"),
            ('use sqlx as db; db::query("SELECT 1");', "query"),
            ('use sqlx as r#db; r#db::r#query("SELECT 1");', "query"),
            ('use sqlx::{self as db}; db::query("SELECT 1");', "query"),
            ('use db::query as q; use sqlx as db; q("SELECT 1");', "query"),
            ('extern crate sqlx as db; db::query("SELECT 1");', "query"),
            ('use sqlx::*; query("SELECT 1");', "query"),
            ('sqlx::raw_sql("SELECT 1");', "raw_sql"),
            ('sqlx::QueryBuilder::<sqlx::Postgres>::new("SELECT 1");', "QueryBuilder::new"),
            ('sqlx::query_builder::QueryBuilder::new("SELECT 1");', "QueryBuilder::new"),
            ('use sqlx::QueryBuilder as B; B::new("SELECT 1");', "QueryBuilder::new"),
            ('let q = sqlx::query; q("SELECT 1");', "query.reference"),
            ('type B = sqlx::QueryBuilder<\'static, sqlx::Postgres>;', "QueryBuilder.reference"),
            ('pool.execute(sql).await;', "Executor.execute"),
            ('pool.fetch_one(sql).await;', "Executor.fetch_one"),
            ('pool.prepare(sql).await;', "Executor.prepare"),
            ('Executor::execute(pool, sql).await;', "Executor.execute"),
            ('let run = Executor::execute;', "Executor.execute.reference"),
            ('< &PgPool as Executor >::fetch(pool, sql);', "Executor.fetch"),
        ]
        for source, expected in cases:
            with self.subTest(source=source):
                self.assertEqual([site.rsplit(":", 1)[0] for site in guard.calls(source)], [expected])

    def test_sqlx_type_does_not_consume_later_queries_or_other_constructors(self):
        source = '''fn h(pool: sqlx::postgres::PgPool) {
            sqlx::query("SELECT 1");
            let unrelated = String::new();
            sqlx::query("SELECT 2");
        }'''
        self.assertEqual([site.rsplit(":", 1)[0] for site in guard.calls(source)], ["query", "query"])
        self.assertEqual(guard.calls("let pool = sqlx::postgres::PgPoolOptions::new();"), [])

    def test_lifetimes_do_not_consume_queries(self):
        source = '''fn h() -> &'static str {
            sqlx::query("SELECT 1");
            "ok"
        }'''
        self.assertEqual(len(guard.calls(source)), 1)

    def test_comments_and_literals_are_not_code(self):
        cases = [
            '// sqlx::query("SELECT 1");',
            '/* outer /* inner */ sqlx::query("SELECT 1"); */',
            'let s = "sqlx::query(\\\"SELECT 1\\\")";',
            'let s = r#"a " sqlx::query("SELECT 1") " b"#;',
            'let s = br##" a "# sqlx::query("SELECT 1") "##;',
            'let c = \'"\'; let s = "sqlx::query";',
        ]
        for source in cases:
            with self.subTest(source=source):
                self.assertEqual(guard.calls(source), [])

    def test_fingerprints_preserve_sql_but_ignore_rust_whitespace_and_comments(self):
        original = guard.calls('sqlx::query("SELECT 1");')
        self.assertEqual(original, guard.calls('sqlx /* nested /* ok */ */ :: query ( "SELECT 1" );'))
        self.assertNotEqual(original, guard.calls('sqlx::query("SELECT 2");'))
        self.assertNotEqual(guard.calls('sqlx::query!(r#"SELECT ")""#);'), [])

    def test_baseline_rejects_replacement_duplicate_and_removal(self):
        original = guard.calls('sqlx::query("SELECT 1");')
        expected = {"handler.rs": original}
        self.assertEqual(guard.check(expected, expected), [])
        for changed in (guard.calls('sqlx::query("SELECT 2");'), original * 2, []):
            with self.subTest(changed=changed):
                self.assertTrue(guard.check({"handler.rs": changed}, expected))

    def test_main_rejects_new_query_in_a_new_handler(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            handlers = root / "handlers"
            handlers.mkdir()
            baseline = root / "baseline.json"
            baseline.write_text(json.dumps({}))
            with patch.multiple(guard, ROOT=root, HANDLERS=handlers, BASELINE=baseline):
                with patch("sys.argv", ["check_handler_sql.py"]), contextlib.redirect_stdout(io.StringIO()):
                    self.assertEqual(guard.main(), 0)
                    (handlers / "new.rs").write_text('sqlx::query("SELECT 1");')
                    errors = io.StringIO()
                    with contextlib.redirect_stderr(errors):
                        self.assertEqual(guard.main(), 1)
                    self.assertIn("handlers/new.rs: new or changed SQL", errors.getvalue())


if __name__ == "__main__":
    unittest.main()
