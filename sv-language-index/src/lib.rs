use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use bimap::BiHashMap;
use generational_arena::Arena;

use position::{DocumentPosition, DocumentRange, LineIndex, Position, Range};
use sv_parser::*;

pub mod position;
pub mod semantic;
type ItemId = generational_arena::Index;

pub struct DataPerFile {
    pub line_index: LineIndex,
    items: Arena<semantic::Item>,
    location_map: Vec<(Range, ItemId)>,
    global_items: HashMap<String, ItemId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileId(u16);

#[derive(Default)]
pub struct Db {
    files: BiHashMap<PathBuf, FileId>,
    data: HashMap<FileId, DataPerFile>,
    global_items: HashMap<String, (FileId, ItemId)>,
}

impl Db {
    fn get_fileid(&self, path: &Path) -> Option<FileId> {
        self.files.get_by_left(path).copied()
    }

    fn path_to_fileid(&mut self, path: PathBuf) -> FileId {
        if let Some(id) = self.files.get_by_left(&path) {
            *id
        } else {
            let new_file_id = FileId(self.files.len() as _);
            self.files.insert(path, new_file_id);
            new_file_id
        }
    }

    pub fn update(&mut self, path: PathBuf, syntax_tree: &SyntaxTree) -> FileId {
        let data = DataPerFile::new(syntax_tree);
        let file_id = self.path_to_fileid(path);
        if let Some(old_data) = self.data.insert(file_id, data) {
            for old_module in old_data.global_items.keys() {
                self.global_items.remove(old_module);
            }
        }
        let new_data = unsafe { self.data.get(&file_id).unwrap_unchecked() };
        for (new_module, idx) in &new_data.global_items {
            self.global_items
                .insert(new_module.clone(), (file_id, *idx));
        }

        file_id
    }

    pub fn get_data(&self, file_id: FileId) -> Option<&DataPerFile> {
        self.data.get(&file_id)
    }

    fn get_item_on_location(
        &self,
        file_id: FileId,
        position: &Position,
    ) -> Option<&semantic::Item> {
        self.data.get(&file_id).and_then(|data| {
            let id = data
                .location_map
                .binary_search_by(|(loc, _)| loc.partial_cmp(&position).unwrap());
            id.ok().map(|id| {
                data.items
                    .get(data.location_map.get(id).unwrap().1)
                    .unwrap()
            })
        })
    }

    fn get_module(&self, module_name: &str) -> Option<(FileId, &semantic::Item)> {
        let (file_id, item_id) = self.global_items.get(module_name)?;
        let data = unsafe { self.data.get(file_id).unwrap_unchecked() };
        data.items.get(*item_id).map(|item| (*file_id, item))
    }

    pub fn goto_definition(&self, request_location: DocumentPosition) -> Option<DocumentRange> {
        let file_id = self.get_fileid(&request_location.document)?;
        let semantic = self.get_item_on_location(file_id, &request_location.position)?;
        let (file_id, location) = match semantic {
            semantic::Item::ModuleIdentifier { module_name, .. } => self
                .get_module(module_name)
                .map(|(id, item)| (id, item.location())),
            semantic::Item::ModuleInstance { instance_name, .. } => self
                .data
                .get(&file_id)
                .and_then(|data| data.items.get(*instance_name))
                .map(|item| (file_id, item.location())),
            semantic::Item::UnknownIdentifier { location, .. } => Some((file_id, location)),
        }?;
        let document = self
            .files
            .get_by_right(&file_id)
            .map(|path| path.clone())
            .unwrap();

        Some(DocumentRange {
            document,
            range: location.clone(),
        })
    }
}

trait HasLocate {
    fn locate(&self) -> &Locate;
}

impl HasLocate for Identifier {
    fn locate(&self) -> &Locate {
        match self {
            Identifier::SimpleIdentifier(id) => &id.nodes.0,
            Identifier::EscapedIdentifier(id) => &id.nodes.0,
        }
    }
}

impl HasLocate for ModuleIdentifier {
    fn locate(&self) -> &Locate {
        self.nodes.0.locate()
    }
}

impl HasLocate for InstanceIdentifier {
    fn locate(&self) -> &Locate {
        self.nodes.0.locate()
    }
}

impl DataPerFile {
    fn get_str<'a>(syntax_tree: &'a SyntaxTree, node: RefNode<'a>) -> Option<&'a str> {
        syntax_tree.get_str_trim(RefNodes(vec![node]))
    }

    fn get_location_of_node<N: HasLocate>(&self, node: &N) -> Range {
        let locate = node.locate();
        let begin = self.line_index.locate_to_position(locate);
        let mut end = begin.clone();
        end.col += locate.len as u32;

        Range { begin, end }
    }

    fn insert_semantic(&mut self, semantic: semantic::Item) -> ItemId {
        let location = semantic.location().clone();
        let pos = match self
            .location_map
            .binary_search_by(|(l, _)| l.begin.cmp(&location.begin))
        {
            Err(pos) => pos,
            Ok(pos) => panic!(
                "Already other semantic exists on same position({pos}). new semantic: {semantic:?}"
            ),
        };
        let idx = self.items.insert(semantic);
        self.location_map.insert(pos, (location.clone(), idx));

        idx
    }

    fn process_module_declaration<'a, ITEM: Iterator<Item = &'a NonPortModuleItem>>(
        &mut self,
        syntax_tree: &SyntaxTree,
        module_keyword: &ModuleKeyword,
        identifier: &ModuleIdentifier,
        end_locate: &Locate,
        items: ITEM,
    ) {
        let module_name =
            Self::get_str(&syntax_tree, RefNode::ModuleIdentifier(identifier)).unwrap();

        let locate = match module_keyword {
            ModuleKeyword::Module(keyword) => &keyword.nodes.0,
            ModuleKeyword::Macromodule(keyword) => &keyword.nodes.0,
        };
        let location = Range {
            begin: self.line_index.locate_to_position(locate),
            end: self.line_index.locate_to_position(end_locate),
        };

        let module_id = self.insert_semantic(semantic::Item::ModuleIdentifier {
            module_name: module_name.to_string(),
            location: location,
        });

        for item in items {
            match item {
                NonPortModuleItem::GenerateRegion(_) => todo!(),
                NonPortModuleItem::ModuleOrGenerateItem(item) => match item.as_ref() {
                    ModuleOrGenerateItem::Parameter(_) => todo!(),
                    ModuleOrGenerateItem::Gate(_) => todo!(),
                    ModuleOrGenerateItem::Udp(_) => todo!(),
                    ModuleOrGenerateItem::Module(item) => {
                        let item = &item.nodes.1;
                        let identifier = &item.nodes.0;
                        let module_name =
                            Self::get_str(syntax_tree, RefNode::ModuleIdentifier(identifier))
                                .unwrap()
                                .to_string();
                        let location = self.get_location_of_node(identifier);
                        let module_id = self.insert_semantic(semantic::Item::ModuleIdentifier {
                            module_name,
                            location,
                        });

                        let instance_name_node = &item.nodes.2.nodes.0.nodes.0.nodes.0;
                        let instance_name = Self::get_str(
                            syntax_tree,
                            RefNode::InstanceIdentifier(&instance_name_node),
                        )
                        .unwrap()
                        .to_string();
                        let location = self.get_location_of_node(&instance_name_node.nodes.0);
                        let instance_name =
                            self.insert_semantic(semantic::Item::UnknownIdentifier {
                                name: instance_name,
                                location,
                            });
                        let location =
                            self.get_location_of_node(&item.nodes.2.nodes.0.nodes.0.nodes.0);

                        self.items.insert(semantic::Item::ModuleInstance {
                            module_name: module_id,
                            instance_name,
                            parameters: Vec::new(),
                            ports: Vec::new(),
                            location: location.clone(),
                        });
                    }
                    ModuleOrGenerateItem::ModuleItem(_) => {}
                },
                NonPortModuleItem::SpecifyBlock(_) => todo!(),
                NonPortModuleItem::Specparam(_) => todo!(),
                NonPortModuleItem::ProgramDeclaration(_) => todo!(),
                NonPortModuleItem::ModuleDeclaration(_) => todo!(),
                NonPortModuleItem::InterfaceDeclaration(_) => todo!(),
                NonPortModuleItem::TimeunitsDeclaration(_) => todo!(),
            }
        }

        self.global_items.insert(module_name.to_string(), module_id);
    }

    pub fn new(syntax_tree: &SyntaxTree) -> Self {
        let line_index = LineIndex::new(syntax_tree);

        let mut ret = Self {
            items: Arena::new(),
            location_map: Vec::new(),
            global_items: HashMap::new(),
            line_index,
        };
        for node in syntax_tree {
            match node {
                RefNode::SourceText(source_text) => {
                    eprintln!("{:#?}", source_text);
                    for desc in &source_text.nodes.2 {
                        match desc {
                            Description::ModuleDeclaration(module) => match module.as_ref() {
                                ModuleDeclaration::Nonansi(module) => {
                                    let header = &module.nodes.0;
                                    let end_locate = module.nodes.4.as_ref().map_or_else(
                                        || &module.nodes.3.nodes.0,
                                        |(symbol, _)| &symbol.nodes.0,
                                    );

                                    // header.nodes.6.into_iter().map(|a| a);

                                    let items = module.nodes.2.iter().filter_map(|item| {
                                        if let ModuleItem::NonPortModuleItem(item) = item {
                                            Some(item.as_ref())
                                        } else {
                                            None
                                        }
                                    });

                                    ret.process_module_declaration(
                                        &syntax_tree,
                                        &header.nodes.1,
                                        &header.nodes.3,
                                        end_locate,
                                        items,
                                    );
                                }
                                ModuleDeclaration::Ansi(module) => {
                                    let header = &module.nodes.0;
                                    let end_locate = module.nodes.4.as_ref().map_or_else(
                                        || &module.nodes.3.nodes.0,
                                        |(symbol, _)| &symbol.nodes.0,
                                    );

                                    let items = module.nodes.2.iter();

                                    ret.process_module_declaration(
                                        &syntax_tree,
                                        &header.nodes.1,
                                        &header.nodes.3,
                                        end_locate,
                                        items,
                                    );
                                }
                                ModuleDeclaration::Wildcard(_) => todo!(),
                                ModuleDeclaration::ExternNonansi(_) => todo!(),
                                ModuleDeclaration::ExternAnsi(_) => todo!(),
                            },
                            _ => { /* not yet */ }
                        }
                    }
                }
                _ => { /* do nothing */ }
            };
        }

        for (location, syntax) in &ret.location_map {
            eprintln!("{:?} - {:?}", location, ret.items.get(*syntax));
        }

        ret
    }
}
