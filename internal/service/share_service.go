package service

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"errors"
	"fmt"
	"strings"
	"time"

	"github.com/abdullaabdullazade/freedrive/internal/domain"
	"github.com/abdullaabdullazade/freedrive/internal/repository"
	"golang.org/x/crypto/bcrypt"
)

var (
	ErrShareTargetMissing = errors.New("file_id or folder_id is required")
	ErrShareNotFound      = errors.New("share not found")
	ErrShareLinkInvalid   = errors.New("invalid or expired share link")
)

// ShareService handles sharing business logic.
type ShareService struct {
	shareRepo  repository.ShareRepository
	fileRepo   repository.FileRepository
	folderRepo repository.FolderRepository
	userRepo   repository.UserRepository
	access     *AccessService
}

// NewShareService creates a share service.
func NewShareService(
	shareRepo repository.ShareRepository,
	fileRepo repository.FileRepository,
	folderRepo repository.FolderRepository,
	userRepo repository.UserRepository,
	access *AccessService,
) *ShareService {
	return &ShareService{
		shareRepo:  shareRepo,
		fileRepo:   fileRepo,
		folderRepo: folderRepo,
		userRepo:   userRepo,
		access:     access,
	}
}

// SharedItem is an enriched share listing entry.
type SharedItem struct {
	Share        domain.UserShare `json:"share"`
	ItemType     string           `json:"item_type"`
	ItemID       string           `json:"item_id"`
	ItemName     string           `json:"item_name"`
	OwnerID      string           `json:"owner_id"`
	OwnerName    string           `json:"owner_name,omitempty"`
	OwnerEmail   string           `json:"owner_email,omitempty"`
	SharedByName string           `json:"shared_by_name,omitempty"`
}

// CreateUserShare shares a file or folder with another user.
func (s *ShareService) CreateUserShare(ctx context.Context, actorID string, share *domain.UserShare) (*domain.UserShare, error) {
	if share.FileID == nil && share.FolderID == nil {
		return nil, ErrShareTargetMissing
	}
	if share.SharedWith == "" {
		return nil, fmt.Errorf("shared_with is required")
	}
	share.Permission = ParsePermission(string(share.Permission))
	share.SharedBy = actorID

	if err := s.canManageShares(ctx, actorID, share.FileID, share.FolderID); err != nil {
		return nil, err
	}

	target, err := s.userRepo.GetByID(ctx, share.SharedWith)
	if err != nil || target == nil {
		return nil, fmt.Errorf("recipient not found")
	}

	if err := s.shareRepo.CreateUserShare(ctx, share); err != nil {
		return nil, err
	}
	return share, nil
}

// DeleteUserShare removes a user share if the actor owns the item or created the share.
func (s *ShareService) DeleteUserShare(ctx context.Context, actorID, shareID string) error {
	share, err := s.shareRepo.GetUserShareByID(ctx, shareID)
	if err != nil || share == nil {
		return ErrShareNotFound
	}
	if err := s.canManageShares(ctx, actorID, share.FileID, share.FolderID); err != nil {
		return err
	}
	return s.shareRepo.DeleteUserShare(ctx, shareID)
}

// UpdateUserShare updates permission on an existing user share.
func (s *ShareService) UpdateUserShare(ctx context.Context, actorID, shareID string, permission domain.Permission) (*domain.UserShare, error) {
	share, err := s.shareRepo.GetUserShareByID(ctx, shareID)
	if err != nil || share == nil {
		return nil, ErrShareNotFound
	}
	if err := s.canManageShares(ctx, actorID, share.FileID, share.FolderID); err != nil {
		return nil, err
	}
	share.Permission = ParsePermission(string(permission))
	if err := s.shareRepo.UpdateUserShare(ctx, share); err != nil {
		return nil, err
	}
	return share, nil
}

// ListSharedWithMe returns shares where the user is the recipient.
func (s *ShareService) ListSharedWithMe(ctx context.Context, userID string) ([]SharedItem, error) {
	shares, err := s.shareRepo.ListSharedWithUser(ctx, userID)
	if err != nil {
		return nil, err
	}
	return s.enrichShares(ctx, shares)
}

// ListSharedByMe returns shares created by the user.
func (s *ShareService) ListSharedByMe(ctx context.Context, userID string) ([]SharedItem, error) {
	shares, err := s.shareRepo.ListSharedByUser(ctx, userID)
	if err != nil {
		return nil, err
	}
	return s.enrichShares(ctx, shares)
}

func (s *ShareService) enrichShares(ctx context.Context, shares []domain.UserShare) ([]SharedItem, error) {
	out := make([]SharedItem, 0, len(shares))
	for _, share := range shares {
		item := SharedItem{Share: share}
		if share.FileID != nil {
			file, err := s.fileRepo.GetByID(ctx, *share.FileID)
			if err != nil || file == nil {
				continue
			}
			item.ItemType = "file"
			item.ItemID = file.ID
			item.ItemName = file.Name
			item.OwnerID = file.OwnerID
		}
		if share.FolderID != nil {
			folder, err := s.folderRepo.GetByID(ctx, *share.FolderID)
			if err != nil || folder == nil {
				continue
			}
			item.ItemType = "folder"
			item.ItemID = folder.ID
			item.ItemName = folder.Name
			item.OwnerID = folder.OwnerID
		}
		if owner, _ := s.userRepo.GetByID(ctx, item.OwnerID); owner != nil {
			item.OwnerName = displayUserName(owner.Username, owner.Email)
			item.OwnerEmail = owner.Email
		}
		if sharer, _ := s.userRepo.GetByID(ctx, share.SharedBy); sharer != nil {
			item.SharedByName = displayUserName(sharer.Username, sharer.Email)
		} else if item.OwnerName != "" {
			item.SharedByName = item.OwnerName
		}
		out = append(out, item)
	}
	return out, nil
}

func displayUserName(username, email string) string {
	username = strings.TrimSpace(username)
	if username != "" {
		return username
	}
	return strings.TrimSpace(email)
}

// CreateLink creates a public share link for a file or folder.
func (s *ShareService) CreateLink(ctx context.Context, actorID string, link *domain.ShareLink, password string) (*domain.ShareLink, error) {
	if link.FileID == nil && link.FolderID == nil {
		return nil, ErrShareTargetMissing
	}
	link.Permission = ParsePermission(string(link.Permission))
	link.CreatedBy = actorID
	link.IsActive = true
	link.Token = randomShareToken(32)

	if password != "" {
		hash, err := bcrypt.GenerateFromPassword([]byte(password), bcrypt.DefaultCost)
		if err != nil {
			return nil, err
		}
		link.PasswordHash = string(hash)
	}

	if link.FileID != nil {
		file, err := s.fileRepo.GetByID(ctx, *link.FileID)
		if err != nil || file == nil || file.OwnerID != actorID {
			return nil, fmt.Errorf("access denied")
		}
	}
	if link.FolderID != nil {
		folder, err := s.folderRepo.GetByID(ctx, *link.FolderID)
		if err != nil || folder == nil || folder.OwnerID != actorID {
			return nil, fmt.Errorf("access denied")
		}
	}

	if err := s.shareRepo.CreateLink(ctx, link); err != nil {
		return nil, err
	}
	link.HasPassword = link.PasswordHash != ""
	link.PasswordHash = ""
	return link, nil
}

// DeleteLink removes a share link owned by the actor.
func (s *ShareService) DeleteLink(ctx context.Context, actorID, linkID string) error {
	link, err := s.shareRepo.GetLinkByID(ctx, linkID)
	if err != nil || link == nil {
		return ErrShareNotFound
	}
	if link.CreatedBy != actorID {
		return fmt.Errorf("access denied")
	}
	return s.shareRepo.DeleteLink(ctx, linkID)
}

// ListLinks returns share links created by the user.
func (s *ShareService) ListLinks(ctx context.Context, userID string) ([]domain.ShareLink, error) {
	links, err := s.shareRepo.ListLinksByUser(ctx, userID)
	if err != nil {
		return nil, err
	}
	for i := range links {
		links[i].HasPassword = links[i].PasswordHash != ""
		links[i].PasswordHash = ""
	}
	return links, nil
}

// ResolveLink validates a public share link and optional password.
func (s *ShareService) ResolveLink(ctx context.Context, token, password string) (*domain.ShareLink, error) {
	link, err := s.shareRepo.GetLinkByToken(ctx, token)
	if err != nil || link == nil || !link.IsActive {
		return nil, ErrShareLinkInvalid
	}
	if link.ExpiresAt != nil && time.Now().After(*link.ExpiresAt) {
		return nil, ErrShareLinkInvalid
	}
	if link.MaxDownloads != nil && link.DownloadCount >= *link.MaxDownloads {
		return nil, ErrShareLinkInvalid
	}
	if link.PasswordHash != "" {
		if password == "" || bcrypt.CompareHashAndPassword([]byte(link.PasswordHash), []byte(password)) != nil {
			return nil, ErrShareLinkInvalid
		}
	}
	link.HasPassword = link.PasswordHash != ""
	link.PasswordHash = ""
	return link, nil
}

// RecordLinkDownload increments the download counter for a share link.
func (s *ShareService) RecordLinkDownload(ctx context.Context, linkID string) error {
	return s.shareRepo.IncrementDownloadCount(ctx, linkID)
}

// ShareRecipient is one email added in the share dialog.
type ShareRecipient struct {
	Email      string
	Permission string
}

// ShareDelivery is enough for the handler to email a sign-in link.
type ShareDelivery struct {
	Email    string
	Existing bool
	FileID   string
	FolderID string
}

// DeliverShares creates a user share or a pending invite for each email.
func (s *ShareService) DeliverShares(ctx context.Context, actorID, message string, fileID, folderID *string, recipients []ShareRecipient) (shared, invited int, deliveries []ShareDelivery, err error) {
	if err := s.canManageShares(ctx, actorID, fileID, folderID); err != nil {
		return 0, 0, nil, err
	}
	actor, _ := s.userRepo.GetByID(ctx, actorID)
	actorEmail := ""
	if actor != nil {
		actorEmail = strings.ToLower(strings.TrimSpace(actor.Email))
	}
	for _, rec := range recipients {
		email := strings.ToLower(strings.TrimSpace(rec.Email))
		if email == "" || !strings.Contains(email, "@") {
			continue
		}
		if actorEmail != "" && email == actorEmail {
			continue
		}
		perm := ParsePermission(rec.Permission)
		user, uerr := s.userRepo.GetByEmail(ctx, email)
		if uerr == nil && user != nil {
			existing, _ := s.shareRepo.ListSharedWithUser(ctx, user.ID)
			if current := matchingShare(existing, fileID, folderID); current != nil {
				current.Permission = perm
				if err := s.shareRepo.UpdateUserShare(ctx, current); err != nil {
					return shared, invited, deliveries, err
				}
			} else {
				share := &domain.UserShare{
					FileID:     fileID,
					FolderID:   folderID,
					SharedWith: user.ID,
					Permission: perm,
					SharedBy:   actorID,
				}
				if err := s.shareRepo.CreateUserShare(ctx, share); err != nil {
					return shared, invited, deliveries, err
				}
			}
			shared++
			deliveries = append(deliveries, ShareDelivery{Email: email, Existing: true, FileID: idOrEmpty(fileID), FolderID: idOrEmpty(folderID)})
			continue
		}
		invite := &domain.ShareInvite{
			Email:      email,
			FileID:     fileID,
			FolderID:   folderID,
			SharedBy:   actorID,
			Permission: perm,
			Token:      randomShareToken(16),
			Message:    message,
		}
		if err := s.shareRepo.UpsertShareInvite(ctx, invite); err != nil {
			return shared, invited, deliveries, err
		}
		invited++
		deliveries = append(deliveries, ShareDelivery{Email: email, Existing: false, FileID: idOrEmpty(fileID), FolderID: idOrEmpty(folderID)})
	}
	return shared, invited, deliveries, nil
}

// ClaimPendingShares attaches invites for this email to the user account.
func (s *ShareService) ClaimPendingShares(ctx context.Context, userID, email string) error {
	email = strings.ToLower(strings.TrimSpace(email))
	if userID == "" || email == "" {
		return nil
	}
	invites, err := s.shareRepo.ListUnclaimedInvitesByEmail(ctx, email)
	if err != nil {
		return err
	}
	existing, _ := s.shareRepo.ListSharedWithUser(ctx, userID)
	for _, inv := range invites {
		if inv.SharedBy == userID {
			_ = s.shareRepo.MarkInviteClaimed(ctx, inv.ID)
			continue
		}
		if !alreadyShared(existing, inv.FileID, inv.FolderID) {
			share := &domain.UserShare{
				FileID:     inv.FileID,
				FolderID:   inv.FolderID,
				SharedBy:   inv.SharedBy,
				SharedWith: userID,
				Permission: inv.Permission,
			}
			if err := s.shareRepo.CreateUserShare(ctx, share); err != nil {
				return err
			}
		}
		if err := s.shareRepo.MarkInviteClaimed(ctx, inv.ID); err != nil {
			return err
		}
	}
	return nil
}

// ItemSettings returns stored share dialog flags, or defaults.
func (s *ShareService) ItemSettings(ctx context.Context, fileID, folderID string) (*domain.ShareItemSettings, error) {
	return s.shareRepo.GetShareItemSettings(ctx, fileID, folderID)
}

// SaveItemSettings persists share dialog flags. Owner or allowed editor.
func (s *ShareService) SaveItemSettings(ctx context.Context, actorID, fileID, folderID string, settings domain.ShareItemSettings) error {
	var filePtr, folderPtr *string
	if fileID != "" {
		filePtr = &fileID
	}
	if folderID != "" {
		folderPtr = &folderID
	}
	if err := s.canManageShares(ctx, actorID, filePtr, folderPtr); err != nil {
		return err
	}
	return s.shareRepo.SaveShareItemSettings(ctx, fileID, folderID, settings)
}

// WebAccess reports whether this user may share or download the item in the web panel.
func (s *ShareService) WebAccess(ctx context.Context, userID, fileID, folderID string) (canShare, canDownload bool, err error) {
	settings, err := s.shareRepo.GetShareItemSettings(ctx, fileID, folderID)
	if err != nil {
		return false, false, err
	}
	ownerID := ""
	var perm domain.Permission
	if fileID != "" {
		file, ferr := s.fileRepo.GetByID(ctx, fileID)
		if ferr != nil || file == nil {
			return false, false, fmt.Errorf("not found")
		}
		ownerID = file.OwnerID
		p, perr := s.access.FilePermission(ctx, fileID, userID)
		if perr != nil {
			return false, false, perr
		}
		perm = p
	} else if folderID != "" {
		folder, ferr := s.folderRepo.GetByID(ctx, folderID)
		if ferr != nil || folder == nil {
			return false, false, fmt.Errorf("not found")
		}
		ownerID = folder.OwnerID
		p, perr := s.access.FolderPermission(ctx, folderID, userID)
		if perr != nil {
			return false, false, perr
		}
		perm = p
	}
	if userID == ownerID {
		return true, true, nil
	}
	isEditor := permissionRank(perm) >= permissionRank(domain.PermWrite)
	if isEditor {
		return settings.EditorsCanShare, settings.EditorsCanDownload, nil
	}
	return false, settings.ViewersCanDownload, nil
}

func (s *ShareService) canManageShares(ctx context.Context, actorID string, fileID, folderID *string) error {
	if fileID != nil && *fileID != "" {
		file, err := s.fileRepo.GetByID(ctx, *fileID)
		if err != nil || file == nil {
			return fmt.Errorf("access denied")
		}
		if file.OwnerID == actorID {
			return nil
		}
		settings, _ := s.shareRepo.GetShareItemSettings(ctx, *fileID, "")
		if settings != nil && settings.EditorsCanShare && s.access.CanWriteFile(ctx, *fileID, actorID) == nil {
			return nil
		}
		return fmt.Errorf("access denied")
	}
	if folderID != nil && *folderID != "" {
		folder, err := s.folderRepo.GetByID(ctx, *folderID)
		if err != nil || folder == nil {
			return fmt.Errorf("access denied")
		}
		if folder.OwnerID == actorID {
			return nil
		}
		settings, _ := s.shareRepo.GetShareItemSettings(ctx, "", *folderID)
		if settings != nil && settings.EditorsCanShare && s.access.CanWriteFolder(ctx, *folderID, actorID) == nil {
			return nil
		}
		return fmt.Errorf("access denied")
	}
	return ErrShareTargetMissing
}

func matchingShare(existing []domain.UserShare, fileID, folderID *string) *domain.UserShare {
	for i := range existing {
		share := &existing[i]
		if fileID != nil && share.FileID != nil && *share.FileID == *fileID {
			return share
		}
		if folderID != nil && share.FolderID != nil && *share.FolderID == *folderID {
			return share
		}
	}
	return nil
}

func alreadyShared(existing []domain.UserShare, fileID, folderID *string) bool {
	for _, share := range existing {
		if fileID != nil && share.FileID != nil && *share.FileID == *fileID {
			return true
		}
		if folderID != nil && share.FolderID != nil && *share.FolderID == *folderID {
			return true
		}
	}
	return false
}

func idOrEmpty(id *string) string {
	if id == nil {
		return ""
	}
	return *id
}

func randomShareToken(n int) string {
	b := make([]byte, n)
	_, _ = rand.Read(b)
	return hex.EncodeToString(b)
}
