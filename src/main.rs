use std::env;
use std::error::Error;
use std::fs::File;
use std::io::prelude::*;
use std::io::ErrorKind;

#[derive(PartialEq, Copy, Clone, Debug)]
enum Token {
  Plus,
  Minus,
  Right,
  Left,
  PutChar,
  ReadChar,
  JumpIfZero,
  JumpIfNonZero,
}

#[derive(Copy, Clone, Debug)]
struct Inst {
  typ: Token,
  argument: usize,
}

type LabelStack = Vec<usize>;

fn label_push(stack: &mut LabelStack) -> usize {
  let last = match stack.last() {
    Some(&last) => last,
    None => 0,
  };
  stack.push(last + 1);
  last + 1
}

impl Inst {
  fn to_bytecode(&self, loop_stack: &mut LabelStack) -> String {
    let arg = self.argument as i32;
    match self.typ {
      Token::Plus => bytecode::plus(arg),
      Token::Minus => bytecode::plus(-arg),
      Token::Left => bytecode::mov(-arg),
      Token::Right => bytecode::mov(arg),
      Token::PutChar => bytecode::out(),
      Token::ReadChar => bytecode::input(),
      Token::JumpIfZero => bytecode::loop_start(loop_stack),
      Token::JumpIfNonZero => bytecode::loop_end(loop_stack),
    }
  }
}

mod bytecode {

  pub fn plus(count: i32) -> String {
    vec![
      "aload_2".to_string(),
      "iload_1".to_string(),
      "dup2".to_string(),
      "iaload".to_string(),
      format!("bipush {}", count),
      "iadd".to_string(),
      "iastore".to_string(),
    ]
    .join("\n")
  }

  pub fn mov(count: i32) -> String {
    format!("iinc 1 {}", count)
  }

  pub fn out() -> String {
    vec![
      "getstatic java/lang/System/out Ljava/io/PrintStream;".to_string(),
      "aload_2".to_string(),
      "iload_1".to_string(),
      "iaload".to_string(),
      "i2c".to_string(),
      "invokevirtual java/io/PrintStream/print(C)V".to_string(),
    ]
    .join("\n")
  }

  pub fn input() -> String {
    vec![
      "aload_2".to_string(),
      "iload_1".to_string(),
      "getstatic java/lang/System/in Ljava/io/InputStream;".to_string(),
      "invokevirtual java/io/InputStream/read()I".to_string(),
      "iastore".to_string(),
    ]
    .join("\n")
  }

  pub fn loop_start(stack: &mut super::LabelStack) -> String {
    let pos = super::label_push(stack);
    vec![
      format!("loop{}Start:", pos),
      "aload_2".to_string(),
      "iload_1".to_string(),
      "iaload".to_string(),
      format!("ifeq loop{}End", pos),
    ]
    .join("\n")
  }

  pub fn loop_end(stack: &mut super::LabelStack) -> String {
    let pos = stack.pop().unwrap();
    vec![format!("goto loop{}Start", pos), format!("loop{}End:", pos)].join("\n")
  }
}
fn lex_program(program: String) -> Result<Vec<Token>, String> {
  let mut tokens = Vec::new();
  for c in program.chars() {
    match c {
      '+' => tokens.push(Token::Plus),
      '-' => tokens.push(Token::Minus),
      '>' => tokens.push(Token::Right),
      '<' => tokens.push(Token::Left),
      '.' => tokens.push(Token::PutChar),
      ',' => tokens.push(Token::ReadChar),
      '[' => tokens.push(Token::JumpIfZero),
      ']' => tokens.push(Token::JumpIfNonZero),
      _ => (), // skip
    }
  }
  Ok(tokens)
}

fn parse_program(program: Vec<Token>) -> Result<Vec<Inst>, String> {
  let mut pos = 0;
  let mut instructions = Vec::new();
  let mut stack = Vec::new();
  while pos < program.len() {
    let curr = program[pos];
    match curr {
      Token::Plus => instructions.push(compile_foldable(Token::Plus, &mut pos, &program)),
      Token::Minus => instructions.push(compile_foldable(Token::Minus, &mut pos, &program)),
      Token::Right => instructions.push(compile_foldable(Token::Right, &mut pos, &program)),
      Token::Left => instructions.push(compile_foldable(Token::Left, &mut pos, &program)),
      Token::PutChar => instructions.push(compile_foldable(Token::PutChar, &mut pos, &program)),
      Token::ReadChar => instructions.push(compile_foldable(Token::ReadChar, &mut pos, &program)),
      Token::JumpIfZero => {
        stack.push(instructions.len());
        instructions.push(Inst {
          typ: Token::JumpIfZero,
          argument: 0,
        });
      }
      Token::JumpIfNonZero => {
        let open_inst_ptr = stack.pop().unwrap();
        let mut open_inst = instructions[open_inst_ptr];
        open_inst.argument = instructions.len();
        instructions.push(Inst {
          typ: Token::JumpIfNonZero,
          argument: open_inst_ptr,
        });
        instructions[open_inst_ptr] = open_inst;
      }
    }
    pos += 1;
  }
  Ok(instructions)
}

fn compile_foldable(token: Token, pos: &mut usize, program: &Vec<Token>) -> Inst {
  let mut count = 1;
  while *pos < program.len() - 1 && program[*pos + 1] == token {
    count += 1;
    *pos += 1;
  }
  Inst {
    typ: token,
    argument: count,
  }
}

const HEADER: &str = "
.class public Main
.super java/lang/Object

.method public <init>()V
    aload_0
    invokenonvirtual java/lang/Object/<init>()V
    return
.end method

.method public static main([Ljava/lang/String;)V
    .limit stack 10
    .limit locals 3

    iconst_0
    istore_1

    bipush 100
    newarray int
    astore_2
";

const TAIL: &str = "
    return
.end method
";

fn produce_code(instructions: Vec<Inst>) -> String {
  let mut code = vec![HEADER.to_string()];
  let mut stack: LabelStack = Vec::new();
  for inst in instructions {
    code.push(inst.to_bytecode(&mut stack));
  }
  code.push(TAIL.to_string());
  code.join("\n")
}

fn interpret(program: String) {
  let mut tape: Vec<u8> = vec![0; 100];
  let mut ptr = 0;
  let mut stack = Vec::new();
  let mut is_looping = false;
  let mut inner_loops = 0;
  let mut i = 0;
  let mut output = String::new();

  while i < program.len() {
    let c = program.chars().nth(i).unwrap();
    if is_looping {
      if c == '[' {
        inner_loops += 1;
      }
      if c == ']' {
        if inner_loops == 0 {
          is_looping = false;
        } else {
          inner_loops -= 1;
        }
      }
      continue;
    }
    match c {
      '+' => tape[ptr] += 1,
      '-' => tape[ptr] -= 1,
      '>' => ptr += 1,
      '<' => ptr -= 1,
      '[' => {
        if tape[ptr] == 0 {
          is_looping = true;
        } else {
          stack.push(i);
        }
      }
      ']' => {
        if tape[ptr] != 0 {
          i = *stack.last().unwrap();
        } else {
          stack.pop();
        }
      }
      '.' => output.push(tape[ptr] as char),
      ',' => {
        let mut line = String::new();
        match std::io::stdin().read_line(&mut line) {
          Ok(_) => {
            let c = line.chars().nth(0).unwrap();
            tape[ptr] = c as u8;
          }
          Err(_) => (),
        }
      }
      _ => (),
    }
    i += 1;
  }
  println!("{}", output);
}

fn main() -> Result<(), Box<dyn Error>> {
  if let Some(filename) = env::args().nth(1) {
    let mut file = File::open(filename)?;
    let mut program = String::new();
    file.read_to_string(&mut program)?;
    let tokens = lex_program(program).unwrap();
    let instructions = parse_program(tokens).unwrap();
    let code = produce_code(instructions);
    let mut outfile = File::create("main.j")?;
    write!(outfile, "{}", code)?;
    println!("Compiled code to main.j");
    Ok(())
  } else {
    Err(Box::new(std::io::Error::new(
      ErrorKind::InvalidInput,
      "No input file!",
    )))
  }
}
